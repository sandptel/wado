//! WebSocket handlers for server registration (`/register`) and client join
//! (`/join/:remote_id`). After the handshake both sides enter a pure-forwarding
//! loop — the relay never inspects post-handshake message content.
//!
//! Auth model: a single **Remote ID** is both address and access token. A join
//! is authorized purely by connecting to `/join/:remote_id` with an ID that has
//! a live registered server — no password message. The future hardening step is
//! a confirmation gate: hold the join at `PeerConnected` until the server
//! approves it (new Approve/Deny variants), then send `JoinAccepted`.
//!
//! ## One Remote ID, several daemons
//!
//! A Remote ID names a **pool**: any number of `wado` daemons may register under it, and each
//! joining client is given one of its own. That is how two devices run two independent
//! sessions — separate compositors, separate applications, separate encoders — from one ID a
//! human can remember. A session is a process, so the OS keeps them apart and a segfault in
//! one device's graphics stack cannot reach another's.
//!
//! Assignment is: the instance the client names in `?instance=` **if that one is free** (the
//! device coming back to its own desktop), else the first free one, else **refused with a
//! reason that says the pool is full**. Refusal is a normal outcome once every daemon has a
//! client, and it is reported as loudly as success — a client told nothing cannot tell "no
//! room left" from "the connection is broken".
//!
//! Nothing here ever takes a room from a live client, not even for the same device coming
//! back. That was tried on `2026-09-14`: two devices, each reconnecting when its socket
//! closed, evicted each other 132 times in two minutes — the same failure `issues.md` I17
//! recorded at a slower 18 s period. Contention is answered with a different daemon or a
//! plain refusal, never by stealing.

use std::net::SocketAddr;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{ConnectInfo, Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;
use wado_protocol::relay::{RelayMsg, display_remote_id, normalize_remote_id};

use crate::AppState;

/// How often the relay pings an idle client.
///
/// Comfortably under the ~100 s after which a cloudflared quick tunnel drops an idle
/// connection, and far under any load — one small frame per client per interval.
const KEEPALIVE: std::time::Duration = std::time::Duration::from_secs(30);

// ── Server registration ──────────────────────────────────────────────────────

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

    let (remote_id, display_name) = match serde_json::from_str::<RelayMsg>(&first) {
        Ok(RelayMsg::Register { remote_id, display_name }) => {
            (normalize_remote_id(&remote_id), display_name)
        }
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

    // ── 2. Register in the registry ─────────────────────────────────────────
    // Infallible now: a second daemon on this Remote ID joins the pool rather than colliding
    // with the first. The instance id minted here is this registration's identity for as long
    // as its socket lives, and is what rooms are keyed by.
    let (inbox_tx, mut inbox_rx) = mpsc::channel::<String>(128);
    let instance_id = state.registry.insert(remote_id.clone(), display_name.clone(), addr, inbox_tx);
    let pool_size = state.registry.instances_for(&remote_id).len();

    info!(
        remote_id = %display_remote_id(&remote_id),
        display_name = ?display_name,
        %addr,
        instance = %instance_id,
        pool_size,
        "server registered — pool now holds {pool_size} daemon(s) for this Remote ID"
    );

    // ── 3. Send Registered ack ───────────────────────────────────────────────
    let ack = match serde_json::to_string(&RelayMsg::Registered { remote_id: remote_id.clone() }) {
        Ok(s) => s,
        Err(_) => return,
    };
    if ws_tx.send(Message::Text(ack)).await.is_err() {
        state.registry.remove(&instance_id);
        return;
    }

    // ── 4. Bidirectional message pump ────────────────────────────────────────
    // Spawn a task that drains inbox_rx → ws_tx (messages from relay/client to server).
    let instance_fwd = instance_id.clone();
    let _fwd_task = AbortOnDrop(tokio::spawn(async move {
        while let Some(text) = inbox_rx.recv().await {
            if ws_tx.send(Message::Text(text)).await.is_err() {
                debug!(instance = %instance_fwd, "forward task: ws write failed");
                break;
            }
        }
    }));

    // Main task: ws_rx → route to active room's client (server → relay → client).
    while let Some(frame) = ws_rx.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                debug!(
                    remote_id = %display_remote_id(&remote_id),
                    "server msg: {}",
                    head(&text)
                );
                // Forward verbatim to *this daemon's* client. Keyed by instance: with a pool,
                // a Remote ID no longer identifies one conversation.
                if !state.rooms.forward_to_client(&instance_id, text.to_string()).await {
                    // No client in the room yet — message is dropped (e.g. server
                    // sent a session event before a client connected).
                    debug!(remote_id = %display_remote_id(&remote_id), "no client in room, dropping message");
                }
            }
            Ok(Message::Ping(_)) => {} // axum auto-replies with Pong
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // ── 5. Cleanup ───────────────────────────────────────────────────────────
    // `_fwd_task` aborts itself on drop — including when this function unwinds. See
    // `AbortOnDrop`.
    state.registry.remove(&instance_id);
    state.rooms.remove(&instance_id);
    info!(
        remote_id = %display_remote_id(&remote_id),
        %addr,
        instance = %instance_id,
        "server disconnected — registry + room cleaned up"
    );
}

// ── Client join ──────────────────────────────────────────────────────────────

/// Query string on `/join/:remote_id`.
#[derive(serde::Deserialize, Default)]
pub struct JoinQuery {
    /// The daemon instance this client used last, if any. A client stores the `instance_id`
    /// from its `JoinAccepted` and hands it back here, which is what returns it to its own
    /// running desktop rather than to whichever daemon happens to be free.
    instance: Option<String>,
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
    // rejected upgrade: the instance hint is an optimisation, and losing it must cost the
    // client its stickiness, not its connection.
    let wanted = query.and_then(|axum::extract::Query(q)| q.instance);
    let peer = peer_ip(&headers, addr);
    ws.on_upgrade(move |socket| join_loop(socket, remote_id, wanted, addr, peer, state))
}

/// The device's own address, as opposed to the socket the relay sees.
///
/// Every client that arrives through the cloudflared tunnel connects from `127.0.0.1`, so the
/// socket address answers "did it come through the tunnel", never "who is it". Cloudflare puts
/// the real one in `CF-Connecting-IP`; a plain reverse proxy uses `X-Forwarded-For`, whose first
/// entry is the originating client. Direct LAN clients have neither and the socket is the truth.
///
/// ponytail: display only — the headers are client-settable, so nothing is authorized on this.
fn peer_ip(headers: &axum::http::HeaderMap, addr: SocketAddr) -> String {
    headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| addr.to_string())
}

async fn join_loop(
    socket: WebSocket,
    remote_id: String,
    wanted_instance: Option<String>,
    addr: SocketAddr,
    peer: String,
    state: AppState,
) {
    let remote_id = normalize_remote_id(&remote_id);
    let (mut ws_tx, mut ws_rx) = socket.split();

    // ── 1. Authenticate purely from the URL path (the Remote ID IS the join) ─
    let pool = state.registry.instances_for(&remote_id);
    if pool.is_empty() {
        warn!(
            client = %peer,
            remote_id = %display_remote_id(&remote_id),
            "join: no server online with this Remote ID"
        );
        send_deny(&mut ws_tx, "no server online with this Remote ID").await;
        return;
    }
    let pool_size = pool.len();

    // ── 2. Pick a daemon from the pool and claim it ──────────────────────────
    let room_id = Uuid::new_v4().to_string();
    let (client_inbox_tx, mut client_inbox_rx) = mpsc::channel::<String>(128);

    // Preference first, then anything free. The preferred instance is the one this device used
    // last, so a reload or a cell handoff comes back to its own windows and its own running
    // programs. When that daemon is busy the device gets a *different* one rather than taking
    // it — which is what stops two devices evicting each other forever.
    let claim = |i: &crate::registry::Instance| {
        state.rooms.claim(&i.instance_id, &remote_id, room_id.clone(), addr, client_inbox_tx.clone())
    };
    let chosen = wanted_instance
        .as_deref()
        .and_then(|w| pool.iter().find(|i| i.instance_id == w))
        .filter(|i| claim(i))
        .map(|i| (i.clone(), "reclaimed"))
        .or_else(|| pool.iter().find(|i| claim(i)).map(|i| (i.clone(), "assigned")));

    let (instance, assignment) = match chosen {
        Some(pair) => pair,
        None => {
            // Refusal is a normal outcome here, not a fault: every daemon in the pool has a
            // client. It carries the numbers that make it actionable, because a bare denial is
            // indistinguishable from a broken connection.
            warn!(
                client = %peer,
                remote_id = %display_remote_id(&remote_id),
                pool_size,
                "join: every daemon in the pool already has a client"
            );
            // The phrase "already has an active connection" is load-bearing, not prose: the
            // client matches it (`OCCUPIED_RE` in `js/relay_link.js`) to tell "the pool is
            // full" from "no daemon is online", and backs off hard instead of knocking every
            // 500 ms. Without it two devices trade the session forever — `issues.md` I17,
            // measured as 18 knocks in 94 s. `scripts/relay-link-check.mjs` reads this file
            // and fails if the wording moves.
            send_deny(
                &mut ws_tx,
                &format!(
                    "every one of the {pool_size} wado session(s) on this Remote ID already \
                     has an active connection. Close one, or start another daemon \
                     (WADO_INSTANCES) to raise the limit."
                ),
            )
            .await;
            return;
        }
    };
    let instance_id = instance.instance_id.clone();
    let server_inbox_tx = instance.inbox_tx.clone();
    let display_name = instance.display_name.clone();
    let pool_busy =
        state.rooms.busy_among(&pool.iter().map(|i| i.instance_id.clone()).collect::<Vec<_>>());

    info!(
        remote_id = %display_remote_id(&remote_id),
        display_name = ?display_name,
        room_id = %room_id,
        instance = %instance_id,
        assignment,
        pool_busy,
        pool_size,
        client = %peer,
        "client joined — {assignment} daemon, {pool_busy} of {pool_size} session(s) in use"
    );

    // ── 3. Notify server of incoming peer ────────────────────────────────────
    // (Future confirmation gate: wait here for the server's Approve/Deny before
    // sending JoinAccepted.)
    let peer_msg = match serde_json::to_string(&RelayMsg::PeerConnected {
        room_id: room_id.clone(),
        client_addr: peer.clone(),
    }) {
        Ok(s) => s,
        Err(_) => {
            state.rooms.remove_if(&instance_id, &room_id);
            return;
        }
    };
    if server_inbox_tx.send(peer_msg).await.is_err() {
        warn!(remote_id = %display_remote_id(&remote_id), "join: server inbox closed right after lookup");
        state.rooms.remove_if(&instance_id, &room_id);
        send_deny(&mut ws_tx, "server disconnected during handshake").await;
        return;
    }

    // ── 4. Send JoinAccepted to client ───────────────────────────────────────
    let accepted = match serde_json::to_string(&RelayMsg::JoinAccepted {
        remote_id: remote_id.clone(),
        room_id: room_id.clone(),
        instance_id: instance_id.clone(),
        pool_size,
        pool_busy,
        assignment: assignment.to_string(),
    }) {
        Ok(s) => s,
        Err(_) => {
            state.rooms.remove_if(&instance_id, &room_id);
            return;
        }
    };
    if ws_tx.send(Message::Text(accepted)).await.is_err() {
        state.rooms.remove_if(&instance_id, &room_id);
        return;
    }

    // ── 4b. Keepalive ────────────────────────────────────────────────────────
    //
    // Once media is flowing this WebSocket carries nothing: video and input are on WebRTC, so
    // the signalling socket sits idle for minutes. The **cloudflared quick tunnel closes an
    // idle connection**, the client reconnects, and the reconnect costs a fresh room, a fresh
    // offer/answer and a black frame — plus four ICE ports on the daemon (I14). Measured
    // `2026-09-14`: one device re-joined every 1-2.5 minutes all evening and renegotiated
    // twice each time, and it read as flaky Wi-Fi rather than an idle socket.
    //
    // The client half of this already shipped — `js/relay_link.js` answers `ping` with `pong`
    // and has since the protocol gained the variants. Only the sender was missing, so nothing
    // needs to be deployed to the client for this to take effect.
    //
    // Sent into the room's own inbox rather than written to the socket here, so it goes
    // through the single forward task that owns `ws_tx` — two writers on one sink is how
    // interleaved frames happen. When the room is dropped the sender closes and this ends.
    let ping_tx = client_inbox_tx.clone();
    let _keepalive = AbortOnDrop(tokio::spawn(async move {
        let mut tick = tokio::time::interval(KEEPALIVE);
        tick.tick().await; // the first tick is immediate; the socket is fresh
        loop {
            tick.tick().await;
            let Ok(text) = serde_json::to_string(&RelayMsg::Ping) else { break };
            if ping_tx.send(text).await.is_err() {
                break; // room gone — nothing to keep alive
            }
        }
    }));

    // ── 5. Bidirectional message pump ────────────────────────────────────────
    // Spawn a task that drains client_inbox_rx → ws_tx (server → relay → client).
    let remote_id_fwd = remote_id.clone();
    let _fwd_task = AbortOnDrop(tokio::spawn(async move {
        while let Some(text) = client_inbox_rx.recv().await {
            if ws_tx.send(Message::Text(text)).await.is_err() {
                debug!(remote_id = %remote_id_fwd, "client fwd task: ws write failed");
                break;
            }
        }
    }));

    // Main task: ws_rx → server's inbox (client → relay → server).
    while let Some(frame) = ws_rx.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                // Swallowed here: a `pong` is this socket's own liveness answer and means
                // nothing to the daemon, which would otherwise reject it as an unknown message
                // and send back a `SessionError`.
                if text.contains("\"pong\"") {
                    continue;
                }
                debug!(
                    remote_id = %display_remote_id(&remote_id),
                    client = %peer,
                    "client msg: {}",
                    head(&text)
                );
                if server_inbox_tx.send(text.to_string()).await.is_err() {
                    info!(remote_id = %display_remote_id(&remote_id), "server inbox closed — ending room");
                    break;
                }
            }
            Ok(Message::Ping(_)) => {}
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // ── 6. Cleanup ───────────────────────────────────────────────────────────
    // `_fwd_task` aborts itself on drop — including when this function unwinds. See
    // `AbortOnDrop`.
    state.rooms.remove_if(&instance_id, &room_id);
    // Tell the server the viewer is gone — and nothing more than that.
    //
    // This used to synthesize `{"type":"session_stop"}`. The reason was real at the time: the
    // server's only teardown hung off the WebRTC peer state, which never reaches Failed/Closed
    // when ICE never completed, so a timed-out client left `session_active` set forever and every
    // later join was refused. `viewer_watchdog` covers that case now, by two clocks that do not
    // depend on any single event.
    //
    // What the synthesized stop cost in the meantime: **any** socket close became an instant
    // teardown. A cell handoff, a screen lock, a tunnel hiccup — each one killed the windows and
    // every application the session had launched, before the 45 s grace period downstream could
    // look at it even once. A viewer going away is not a request to stop.
    if let Ok(text) = serde_json::to_string(&RelayMsg::PeerDisconnected { room_id: room_id.clone() })
    {
        let _ = server_inbox_tx.send(text).await;
    }
    info!(
        remote_id = %display_remote_id(&remote_id),
        room_id = %room_id,
        instance = %instance_id,
        client = %peer,
        "client disconnected — room removed"
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async fn send_error(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &str,
) {
    let msg = serde_json::to_string(&RelayMsg::Error { message: message.to_string() })
        .unwrap_or_else(|_| r#"{"type":"error","message":"internal"}"#.to_string());
    let _ = ws_tx.send(Message::Text(msg)).await;
}

async fn send_deny(
    ws_tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    reason: &str,
) {
    let msg = serde_json::to_string(&RelayMsg::JoinDenied { reason: reason.to_string() })
        .unwrap_or_else(|_| r#"{"type":"join_denied","reason":"internal"}"#.to_string());
    let _ = ws_tx.send(Message::Text(msg)).await;
}

/// First [`LOG_HEAD`] *characters* of a relayed message, for the debug log.
///
/// Not `&text[..120]`. That slices on a **byte** index, and a WebSocket frame here carries
/// arbitrary UTF-8 — PTY output most of all. On 2026-09-12 a shell rendering `●` (three bytes)
/// straddling byte 120 panicked the tokio worker handling that room mid-frame. The failure did
/// not look like a panic from outside: the socket simply stopped being drained, 2.6 MB backed
/// up in the daemon's send queue, and every session-start and SDP message after it was never
/// read. The symptom reported was "the shell works but nothing streams" — the shell being
/// precisely what put the `●` on the wire.
///
/// Truncating by characters cannot land mid-codepoint, so it cannot panic.
fn head(text: &str) -> String {
    const LOG_HEAD: usize = 120;
    text.chars().take(LOG_HEAD).collect()
}

#[cfg(test)]
mod tests {
    use super::head;

    #[test]
    fn head_never_splits_a_codepoint() {
        // The exact shape that panicked: 119 ASCII bytes, then a 3-byte char occupying bytes
        // 119..122 — so the old `&text[..120]` cut straight through it.
        let s = format!("{}●tail", "a".repeat(119));
        assert!(!s.is_char_boundary(120), "test no longer reproduces the original panic");
        assert_eq!(head(&s).chars().count(), 120);
        assert!(head(&s).ends_with('●'));
        // Shorter than the cap, empty, and all-multibyte all pass through unharmed.
        assert_eq!(head("hi"), "hi");
        assert_eq!(head(""), "");
        assert_eq!(head(&"●".repeat(200)).chars().count(), 120);
    }
}

/// Aborts a spawned task when this guard is dropped — **including on an unwind**.
///
/// The relay's per-connection loops each spawn a forwarder that owns the WebSocket's write
/// half, and each called `fwd_task.abort()` at the end. A panic unwinds *past* that line, so
/// the forwarder survived, kept the socket open, and nothing drained the read half: 2.6 MB
/// backed up in the daemon's send queue and every message after the panic was silently lost.
/// The visible symptom was "the shell works but nothing streams" — nobody would look for a
/// panic, because the process was still up and still serving other rooms.
///
/// A guard turns that into what it should have been: the connection closes, the peer notices,
/// and it reconnects.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
