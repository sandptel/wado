//! `/register` — one daemon's connection to the relay.
//!
//! The daemon registers, gets `registered` back, and from then on everything it sends is
//! forwarded to the client on its seat — except the frames addressed to the relay itself:
//! its `pong`s, and its verdicts on gated joins.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use wado_protocol::relay_wire::{display_remote_id, normalize_remote_id, WireMsg, WIRE_VERSION};

use crate::registry::NewServer;
use crate::signaling::{
    caps, head, is_pong, send_error, send_wire, AbortOnDrop, KEEPALIVE, SILENCE,
};
use crate::AppState;

/// The longest seat hold a daemon may ask for. Bounds how long a gone client's seat can occupy
/// the relay's memory; well above the daemon's own 30-minute grace.
const MAX_HOLD: Duration = Duration::from_secs(4 * 3600);

pub async fn handle_register(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| register_loop(socket, addr, state))
}

async fn register_loop(socket: WebSocket, addr: SocketAddr, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // ── 1. Expect the first message to be Register ──────────────────────────
    let first = match ws_rx.next().await {
        Some(Ok(Message::Text(t))) => t,
        _ => {
            warn!(%addr, "register: no first message or non-text frame");
            return;
        }
    };
    let (remote_id, display_name, peer_v, instance_key, boot_id, hold_ms, peer_caps) =
        match serde_json::from_str::<WireMsg>(&first) {
            Ok(WireMsg::Register {
                remote_id,
                display_name,
                v,
                instance_key,
                boot_id,
                hold_ms,
                caps,
            }) => (
                normalize_remote_id(&remote_id),
                display_name,
                v,
                instance_key,
                boot_id,
                hold_ms,
                caps,
            ),
            Ok(other) => {
                warn!(%addr, ?other, "register: expected Register, got something else");
                send_error(&mut ws_tx, "expected Register as first message").await;
                return;
            }
            Err(e) => {
                warn!(%addr, %e, "register: bad JSON");
                send_error(&mut ws_tx, "malformed Register message").await;
                return;
            }
        };
    if remote_id.is_empty() {
        warn!(%addr, "register: empty Remote ID");
        send_error(&mut ws_tx, "Remote ID must not be empty").await;
        return;
    }
    let has = |c: &str| peer_caps.iter().any(|p| p == c);
    // Only a daemon that answers pings may be timed out for not answering them — an older one
    // would be cut off every `SILENCE` for doing nothing wrong.
    let pongs = has("pong");

    // ── 2. Register in the registry ─────────────────────────────────────────
    // A second daemon on this Remote ID joins the pool. The instance id is stable when the
    // daemon sends an `instance_key`, which is what returns a client to the same daemon after
    // the relay restarts; see `ServerRegistry::insert` for the one case that is refused.
    let (inbox_tx, mut inbox_rx) = mpsc::channel::<String>(128);
    let ping_tx = inbox_tx.clone();
    let reg = match state.registry.insert(NewServer {
        remote_id: remote_id.clone(),
        instance_key,
        boot_id,
        display_name: display_name.clone(),
        addr,
        inbox_tx,
        hold: Duration::from_millis(hold_ms).min(MAX_HOLD),
        gate: has("gate"),
    }) {
        Ok(reg) => reg,
        Err(why) => {
            warn!(remote_id = %display_remote_id(&remote_id), %addr, "register refused: {why}");
            send_error(&mut ws_tx, &why).await;
            return;
        }
    };
    let instance_id = reg.instance_id.clone();
    let mut kicked = reg.kicked;
    let pool_size = state.registry.instances_for(&remote_id).len();
    info!(
        remote_id = %display_remote_id(&remote_id),
        display_name = ?display_name,
        %addr,
        instance = %instance_id,
        pool_size,
        wire_v = peer_v,
        caps = ?peer_caps,
        hold_ms,
        replaced = reg.replaced,
        "server registered — pool now holds {pool_size} daemon(s) for this Remote ID"
    );

    // ── 3. Send Registered ack ───────────────────────────────────────────────
    let ack = WireMsg::Registered {
        remote_id: remote_id.clone(),
        relay_v: WIRE_VERSION,
        caps: caps(),
    };
    if !send_wire(&mut ws_tx, &ack).await {
        state.registry.remove_if(&instance_id, &reg.conn_id);
        return;
    }

    // ── 4. Pump ──────────────────────────────────────────────────────────────
    // Everything bound for the daemon goes through its inbox, so this task is the socket's only
    // writer — two writers on one sink is how interleaved frames happen.
    let instance_fwd = instance_id.clone();
    let _fwd_task = AbortOnDrop(tokio::spawn(async move {
        while let Some(text) = inbox_rx.recv().await {
            if ws_tx.send(Message::Text(text)).await.is_err() {
                debug!(instance = %instance_fwd, "forward task: ws write failed");
                break;
            }
        }
    }));
    let _pinger = pongs.then(|| {
        AbortOnDrop(tokio::spawn(async move {
            let mut tick = tokio::time::interval(KEEPALIVE);
            tick.tick().await;
            loop {
                tick.tick().await;
                let Ok(text) = serde_json::to_string(&WireMsg::Ping) else {
                    break;
                };
                if ping_tx.send(text).await.is_err() {
                    break;
                }
            }
        }))
    });

    let mut last_heard = Instant::now();
    let mut check = tokio::time::interval(Duration::from_secs(5));
    loop {
        let frame = tokio::select! {
            f = ws_rx.next() => f,
            // This registration was replaced by the same daemon redialling. Without this, the
            // stale socket would linger until its TCP died, which behind a tunnel can be never.
            _ = &mut kicked => {
                info!(instance = %instance_id, "server connection replaced by a redial — closing the old one");
                return; // its cleanup belongs to the replacement now
            }
            _ = check.tick() => {
                if pongs && last_heard.elapsed() > SILENCE {
                    warn!(instance = %instance_id, "server silent for {SILENCE:?} — treating it as gone");
                    break;
                }
                continue;
            }
        };
        last_heard = Instant::now();
        match frame {
            Some(Ok(Message::Text(text))) => {
                from_daemon(&state, &instance_id, text.to_string()).await
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            Some(Ok(_)) => {} // axum answers WS-level pings itself
        }
    }

    // ── 5. Cleanup ───────────────────────────────────────────────────────────
    // Guarded: if this connection was replaced mid-teardown, the registration belongs to the
    // replacement. Seats are left alone on purpose — each client's own loop notices the daemon's
    // inbox close, ends its link, and leaves its seat held for when the daemon comes back.
    state.registry.remove_if(&instance_id, &reg.conn_id);
    info!(
        remote_id = %display_remote_id(&remote_id),
        %addr,
        instance = %instance_id,
        "server disconnected"
    );
}

/// One frame from the daemon: for the relay, or for the client on its seat.
async fn from_daemon(state: &AppState, instance_id: &str, text: String) {
    if is_pong(&text) {
        return;
    }
    // Gate verdicts are for the relay. Prefix first, because nearly every frame is something
    // else and serde writes the tag first, so this costs one comparison instead of a parse.
    if text.starts_with(r#"{"type":"peer_"#) {
        match serde_json::from_str::<WireMsg>(&text) {
            Ok(WireMsg::PeerAccept { room_id }) => {
                state.rooms.deliver_verdict(instance_id, &room_id, Ok(()));
                return;
            }
            Ok(WireMsg::PeerReject { room_id, reason }) => {
                state
                    .rooms
                    .deliver_verdict(instance_id, &room_id, Err(reason));
                return;
            }
            _ => {}
        }
    }
    debug!(instance = %instance_id, "server msg: {}", head(&text));
    if !state.rooms.forward_to_client(instance_id, text).await {
        // No accepted client on the seat — dropped (e.g. a session event before anyone joined).
        debug!(instance = %instance_id, "no client on the seat, dropping message");
    }
}
