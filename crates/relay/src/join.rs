//! `/join/:remote_id` — one client's connection to the relay.
//!
//! In order: rate limit → **park** until there is a daemon to go to → claim a seat (its own held
//! seat, a free daemon, or — after a human tap — another device's) → the daemon's **gate**, if
//! it has one → `join_accepted` → pump until either side goes. When the client's socket closes
//! its seat is **held** for it (see [`crate::room`]); when the daemon's goes, the client's link
//! is closed so it comes back through here and parks until the daemon returns.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{ConnectInfo, Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};
use uuid::Uuid;
use wado_protocol::relay_wire::{display_remote_id, normalize_remote_id, WireMsg, WIRE_VERSION};

use crate::registry::Instance;
use crate::room::{Claim, Want};
use crate::signaling::{
    caps, head, is_pong, occupied_reason, peer_ip, send_deny, send_wire, AbortOnDrop, WsTx,
    KEEPALIVE, SILENCE,
};
use crate::AppState;

/// How long a join waits for some device to approve it before it is refused.
const APPROVAL_WAIT: Duration = Duration::from_secs(600);
/// A trusted device is let in within milliseconds. Only a join still waiting after this is told
/// it is waiting for approval — otherwise every reconnect would flash the prompt text.
const APPROVAL_QUIET: Duration = Duration::from_millis(1500);

/// Query string on `/join/:remote_id`. Every field is optional, so older clients join as before.
#[derive(serde::Deserialize, Default)]
pub struct JoinQuery {
    /// The daemon instance this client used last. Handed back so the device returns to *its
    /// own* desktop rather than whichever daemon is free.
    instance: Option<String>,
    /// The browser's random id. What its held seat and the daemon's trust list know it by.
    client: Option<String>,
    /// A label for the device, for other devices' prompts and the logs.
    name: Option<String>,
    /// `1`: a human tapped "use it here" — take the seat even from another device.
    takeover: Option<String>,
}

struct Joiner {
    wanted: Option<String>,
    client_key: String,
    name: String,
    takeover: bool,
}

pub async fn handle_join(
    ws: WebSocketUpgrade,
    Path(remote_id): Path<String>,
    query: Option<axum::extract::Query<JoinQuery>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // `Option<Query<_>>` so a malformed query string is an empty preference rather than a
    // rejected upgrade: everything in it is an optimisation or a label, and losing it must cost
    // the client its stickiness, not its connection.
    let q = query.map(|axum::extract::Query(q)| q).unwrap_or_default();
    let joiner = Joiner {
        wanted: q.instance.filter(|i| !i.is_empty()),
        // A key is a bearer token for the trust list, so it is bounded and never logged.
        client_key: q
            .client
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(64)
            .collect(),
        name: q
            .name
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(48)
            .collect(),
        takeover: q.takeover.as_deref() == Some("1"),
    };
    let peer = peer_ip(&headers, addr, state.config.trust_proxy);
    ws.on_upgrade(move |socket| join_loop(socket, remote_id, joiner, peer, state))
}

async fn join_loop(socket: WebSocket, remote_id: String, j: Joiner, peer: String, state: AppState) {
    let remote_id = normalize_remote_id(&remote_id);
    let (mut ws_tx, mut ws_rx) = socket.split();
    let rid = display_remote_id(&remote_id);
    let label = if j.name.is_empty() {
        peer.clone()
    } else {
        format!("{} ({peer})", j.name)
    };

    // ── 1. Rate limit ────────────────────────────────────────────────────────
    if let Err(wait) = state.limiter.take(&peer) {
        warn!(client = %peer, remote_id = %rid, "join: rate limited for {wait:?}");
        let secs = wait.as_secs().max(1);
        send_deny(
            &mut ws_tx,
            &format!("too many connection attempts — try again in {secs} s"),
            false,
            wait.as_millis() as u64,
        )
        .await;
        return;
    }

    let room_id = Uuid::new_v4().to_string();
    let (client_inbox_tx, mut client_inbox_rx) = mpsc::channel::<String>(128);
    let mut ping = tokio::time::interval(KEEPALIVE);
    ping.tick().await; // the first tick is immediate; the socket is fresh

    // ── 2. Park until there is a daemon to go to, then claim a seat ──────────
    // The client keeps one open socket while it waits instead of knocking on a timer, and is
    // paired the moment a daemon registers — whichever of the two started first.
    let mut changed = state.registry.watch();
    let mut told = String::new();
    let pick = loop {
        changed.borrow_and_update();
        let pool = state.registry.instances_for(&remote_id);
        // This device's own seat, on a daemon that is away right now: wait for that daemon
        // rather than be handed a different, empty desktop.
        let mine_away = j
            .wanted
            .as_deref()
            .filter(|w| !pool.iter().any(|i| i.instance_id == *w))
            .and_then(|w| state.rooms.held_for(w, &j.client_key));
        let park = match mine_away {
            Some(left) => Some((
                "your computer is reconnecting — your desktop is being held for you",
                left,
            )),
            None if pool.is_empty() => Some((
                "no computer is online with this Remote ID yet — waiting for it to start",
                Duration::ZERO,
            )),
            None => None,
        };
        let Some((reason, left)) = park else {
            match assign(&state, &pool, &j, &room_id, &client_inbox_tx) {
                Ok((i, c, a)) => break Pick::Seat(i, c, a, pool.len()),
                // Every seat is taken, and a human tapped "use it here": take this device's own
                // last daemon if it is in the pool, else the first.
                Err(_) if j.takeover => {
                    let target = j
                        .wanted
                        .as_deref()
                        .and_then(|w| pool.iter().find(|i| i.instance_id == w))
                        .unwrap_or(&pool[0]);
                    break Pick::Takeover(target.clone(), pool.len());
                }
                Err(holders) => {
                    warn!(client = %label, remote_id = %rid, pool_size = pool.len(), "join: every daemon in the pool is in use");
                    send_deny(&mut ws_tx, &occupied_reason(pool.len(), &holders), true, 0).await;
                    return;
                }
            }
        };
        if told != reason {
            info!(client = %label, remote_id = %rid, "join parked: {reason}");
            let msg = WireMsg::Waiting {
                reason: reason.to_string(),
                ms_left: left.as_millis() as u64,
            };
            if !send_wire(&mut ws_tx, &msg).await {
                return;
            }
            told = reason.to_string();
        }
        // A held seat running out is a change too: after it, this client is an ordinary join.
        let wake = if left.is_zero() {
            Duration::from_secs(3600)
        } else {
            left
        };
        tokio::select! {
            _ = changed.changed() => {}
            _ = tokio::time::sleep(wake) => {}
            _ = ping.tick() => {
                if !send_wire(&mut ws_tx, &WireMsg::Ping).await {
                    return;
                }
            }
            f = ws_rx.next() => match f {
                Some(Ok(Message::Text(_))) | Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                _ => {
                    debug!(client = %label, "parked join closed by the client");
                    return;
                }
            },
        }
    };
    let want = |i: &Instance| Want {
        room_id: room_id.clone(),
        client_key: j.client_key.clone(),
        client_name: j.name.clone(),
        inbox_tx: client_inbox_tx.clone(),
        hold: i.hold,
        takeover: true,
    };
    let (instance, claim, assignment, pool_size) = match pick {
        Pick::Seat(i, c, a, n) => (i, c, a, n),
        // A takeover asks the daemon *before* it displaces anyone. Otherwise anyone holding the
        // Remote ID could knock a live viewer off its desktop, and the one device that could
        // have approved them would be the device just told not to reconnect. A trusted device
        // passes at once; an unknown one is put to the viewer it would displace.
        Pick::Takeover(t, n) => {
            if t.gate {
                let verdict = state.rooms.await_check(&room_id);
                let ask = WireMsg::PeerCheck {
                    room_id: room_id.clone(),
                    client_addr: peer.clone(),
                    client_key: j.client_key.clone(),
                    client_name: j.name.clone(),
                };
                send_to(&t.inbox_tx, &ask).await;
                let v = wait_verdict(
                    &mut ws_tx,
                    &mut ws_rx,
                    &mut ping,
                    &t.inbox_tx,
                    verdict,
                    None,
                    &label,
                    &t.instance_id,
                )
                .await;
                state.rooms.drop_check(&room_id);
                if let Gate::Out(reason) = v {
                    // Lets the daemon withdraw the approval request it may have posted.
                    send_to(
                        &t.inbox_tx,
                        &WireMsg::PeerDisconnected {
                            room_id: room_id.clone(),
                        },
                    )
                    .await;
                    if !reason.is_empty() {
                        warn!(client = %label, instance = %t.instance_id, "takeover refused: {reason}");
                        send_deny(&mut ws_tx, &reason, false, 0).await;
                    }
                    return;
                }
            }
            let c = state.rooms.claim(&t.instance_id, want(&t));
            (t, c, "taken over", n)
        }
    };
    let Claim::Got {
        mut kicked,
        displaced,
        ..
    } = claim
    else {
        // The seat changed hands between the check and the claim; the client simply retries.
        return;
    };
    let instance_id = instance.instance_id.clone();
    let server_inbox_tx = instance.inbox_tx.clone();

    // ── 3. Tell the daemon ───────────────────────────────────────────────────
    // A takeover's displaced viewer is reported gone first, so the daemon never sees two.
    if let Some(old) = displaced {
        send_to(
            &server_inbox_tx,
            &WireMsg::PeerDisconnected { room_id: old },
        )
        .await;
    }
    let hello = WireMsg::PeerConnected {
        room_id: room_id.clone(),
        client_addr: peer.clone(),
        client_key: j.client_key.clone(),
        client_name: j.name.clone(),
    };
    if !send_to(&server_inbox_tx, &hello).await {
        state.rooms.remove_if(&instance_id, &room_id);
        send_deny(&mut ws_tx, "server disconnected during handshake", false, 0).await;
        return;
    }

    // ── 4. The daemon's gate ─────────────────────────────────────────────────
    if instance.gate {
        let Some(verdict) = state.rooms.await_verdict(&instance_id, &room_id) else {
            return;
        };
        let v = wait_verdict(
            &mut ws_tx,
            &mut ws_rx,
            &mut ping,
            &server_inbox_tx,
            verdict,
            Some(&mut kicked),
            &label,
            &instance_id,
        )
        .await;
        match v {
            Gate::In => {}
            Gate::Kicked => return,
            Gate::Out(reason) => {
                // Never held: a device that was not let in has no seat to come back to.
                state.rooms.remove_if(&instance_id, &room_id);
                send_to(
                    &server_inbox_tx,
                    &WireMsg::PeerDisconnected {
                        room_id: room_id.clone(),
                    },
                )
                .await;
                if !reason.is_empty() {
                    warn!(client = %label, instance = %instance_id, "join refused: {reason}");
                    send_deny(&mut ws_tx, &reason, false, 0).await;
                }
                return;
            }
        }
    }
    state.rooms.accept(&instance_id, &room_id);

    // ── 5. Accepted ──────────────────────────────────────────────────────────
    let pool_busy = state.rooms.busy_among(
        &state
            .registry
            .instances_for(&remote_id)
            .iter()
            .map(|i| i.instance_id.clone())
            .collect::<Vec<_>>(),
    );
    info!(
        remote_id = %rid,
        room_id = %room_id,
        instance = %instance_id,
        assignment,
        pool_busy,
        pool_size,
        client = %label,
        "client joined — {assignment} daemon, {pool_busy} of {pool_size} session(s) in use"
    );
    let accepted = WireMsg::JoinAccepted {
        remote_id: remote_id.clone(),
        room_id: room_id.clone(),
        instance_id: instance_id.clone(),
        pool_size,
        pool_busy,
        assignment: assignment.to_string(),
        boot_id: instance.boot_id.clone(),
        relay_v: WIRE_VERSION,
        caps: caps(),
    };
    if !send_wire(&mut ws_tx, &accepted).await {
        state.rooms.release(&instance_id, &room_id);
        return;
    }

    // ── 6. Keepalive + pump ──────────────────────────────────────────────────
    //
    // Once media is flowing this WebSocket carries nothing: video and input are on WebRTC, so
    // the socket sits idle for minutes, and a cloudflared quick tunnel closes an idle connection
    // (measured `2026-09-14`: one device re-joined every 1-2.5 minutes all evening). The pings
    // also measure the client: one that has answered nothing for `SILENCE` is gone, and its
    // socket is closed so its seat can be held for it.
    //
    // Pings go into the client's own inbox rather than straight to the socket, so the single
    // forward task stays the socket's only writer.
    let ping_tx = client_inbox_tx.clone();
    let keepalive = AbortOnDrop(tokio::spawn(async move {
        loop {
            ping.tick().await;
            let Ok(text) = serde_json::to_string(&WireMsg::Ping) else {
                break;
            };
            if ping_tx.send(text).await.is_err() {
                break;
            }
        }
    }));
    let rid_fwd = rid.clone();
    let mut fwd = AbortOnDrop(tokio::spawn(async move {
        while let Some(text) = client_inbox_rx.recv().await {
            if ws_tx.send(Message::Text(text)).await.is_err() {
                debug!(remote_id = %rid_fwd, "client fwd task: ws write failed");
                break;
            }
        }
    }));

    let mut last_heard = Instant::now();
    let mut check = tokio::time::interval(Duration::from_secs(5));
    loop {
        let frame = tokio::select! {
            f = ws_rx.next() => f,
            // The daemon went away or was replaced by a redial. The client's link is closed so
            // it comes back through `/join`, where it parks until its daemon returns.
            _ = server_inbox_tx.closed() => {
                info!(instance = %instance_id, client = %label, "daemon gone — closing the client's link; its seat is held");
                break;
            }
            // Another device took this seat with a tap. The `taken_over` notice is already in
            // the inbox: let the forward task deliver it, then close.
            _ = &mut kicked => {
                info!(instance = %instance_id, client = %label, "seat taken over by another device");
                drop(keepalive);
                drop(client_inbox_tx);
                let _ = tokio::time::timeout(Duration::from_secs(2), &mut fwd.0).await;
                return; // the seat is the newcomer's; nothing of ours to release
            }
            _ = check.tick() => {
                if last_heard.elapsed() > SILENCE {
                    warn!(instance = %instance_id, client = %label, "client silent for {SILENCE:?} — closing; its seat is held");
                    break;
                }
                continue;
            }
        };
        last_heard = Instant::now();
        match frame {
            Some(Ok(Message::Text(text))) => {
                // A pong is this socket's own liveness answer and means nothing to the daemon,
                // which would otherwise reject it as an unknown message.
                if is_pong(&text) {
                    continue;
                }
                debug!(remote_id = %rid, client = %peer, "client msg: {}", head(&text));
                if server_inbox_tx.send(text.to_string()).await.is_err() {
                    info!(remote_id = %rid, "server inbox closed — ending room");
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            Some(Ok(_)) => {}
        }
    }

    // ── 7. Cleanup ───────────────────────────────────────────────────────────
    // The seat is *held*, not freed: a viewer going away is not a request to stop. The daemon
    // is told only that the viewer is gone — once this used to synthesize a `session_stop`,
    // and every dropped socket cost the viewer their windows and applications.
    state.rooms.release(&instance_id, &room_id);
    send_to(
        &server_inbox_tx,
        &WireMsg::PeerDisconnected {
            room_id: room_id.clone(),
        },
    )
    .await;
    info!(
        remote_id = %rid,
        room_id = %room_id,
        instance = %instance_id,
        client = %label,
        "client disconnected — seat held"
    );
}

/// Pick a seat: this device's last daemon if it can have it, else the first free one. `Err`
/// carries who holds the pool, for the refusal (or for a takeover, which `join_loop` handles,
/// because it must ask the daemon first).
fn assign(
    state: &AppState,
    pool: &[Instance],
    j: &Joiner,
    room_id: &str,
    inbox: &mpsc::Sender<String>,
) -> Result<(Instance, Claim, &'static str), Vec<String>> {
    let want = |i: &Instance| Want {
        room_id: room_id.to_string(),
        client_key: j.client_key.clone(),
        client_name: j.name.clone(),
        inbox_tx: inbox.clone(),
        hold: i.hold,
        takeover: false,
    };
    let preferred = j
        .wanted
        .as_deref()
        .and_then(|w| pool.iter().find(|i| i.instance_id == w));
    let mut holders = Vec::new();
    let is_preferred = |i: &Instance| preferred.is_some_and(|p| p.instance_id == i.instance_id);
    let others = pool.iter().filter(|i| !is_preferred(i));
    for i in preferred.into_iter().chain(others) {
        match state.rooms.claim(&i.instance_id, want(i)) {
            Claim::Busy { holder, .. } => holders.push(holder),
            got => {
                let how = if is_preferred(i) {
                    "reclaimed"
                } else {
                    "assigned"
                };
                return Ok((i.clone(), got, how));
            }
        }
    }
    Err(holders)
}

/// What the park loop settled on.
enum Pick {
    Seat(Instance, Claim, &'static str, usize),
    Takeover(Instance, usize),
}

enum Gate {
    In,
    /// Refused, with the reason to show — or empty when the client left while waiting.
    Out(String),
    /// Another device took the seat meanwhile.
    Kicked,
}

/// Wait for the daemon's verdict on a join, keeping the client's socket alive and telling it
/// what it is waiting for once the wait is long enough to notice.
#[allow(clippy::too_many_arguments)]
async fn wait_verdict(
    ws_tx: &mut WsTx,
    ws_rx: &mut futures_util::stream::SplitStream<WebSocket>,
    ping: &mut tokio::time::Interval,
    daemon: &mpsc::Sender<String>,
    mut verdict: oneshot::Receiver<Result<(), String>>,
    mut kicked: Option<&mut oneshot::Receiver<()>>,
    label: &str,
    instance_id: &str,
) -> Gate {
    let quiet_until = tokio::time::Instant::now() + APPROVAL_QUIET;
    let give_up = tokio::time::Instant::now() + APPROVAL_WAIT;
    let mut said = false;
    loop {
        tokio::select! {
            v = &mut verdict => return match v {
                Ok(Ok(())) => Gate::In,
                Ok(Err(reason)) => Gate::Out(if reason.is_empty() { "this device was not allowed in".into() } else { reason }),
                Err(_) => Gate::Out("the computer stopped answering".into()),
            },
            _ = tokio::time::sleep_until(quiet_until), if !said => {
                said = true;
                info!(client = %label, instance = %instance_id, "join waiting for approval");
                let msg = WireMsg::Waiting {
                    reason: "waiting for approval — allow this device on one you have used with this computer before".into(),
                    ms_left: APPROVAL_WAIT.as_millis() as u64,
                };
                if !send_wire(ws_tx, &msg).await { return Gate::Out(String::new()); }
            }
            _ = tokio::time::sleep_until(give_up) => return Gate::Out("nobody approved this device in time".into()),
            _ = ping.tick() => {
                if !send_wire(ws_tx, &WireMsg::Ping).await { return Gate::Out(String::new()); }
            }
            _ = daemon.closed() => return Gate::Out("the computer went away".into()),
            _ = async {
                match kicked.as_mut() {
                    Some(k) => { let _ = (&mut **k).await; }
                    None => std::future::pending::<()>().await,
                }
            } => return Gate::Kicked,
            f = ws_rx.next() => match f {
                Some(Ok(Message::Text(_))) | Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                _ => return Gate::Out(String::new()),
            },
        }
    }
}

async fn send_to(inbox: &mpsc::Sender<String>, msg: &WireMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(text) => inbox.send(text).await.is_ok(),
        Err(_) => false,
    }
}
