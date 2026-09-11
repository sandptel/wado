//! WebSocket handlers for server registration (`/register`) and client join
//! (`/join/:remote_id`). After the handshake both sides enter a pure-forwarding
//! loop — the relay never inspects post-handshake message content.
//!
//! Auth model: a single **Remote ID** is both address and access token. A join
//! is authorized purely by connecting to `/join/:remote_id` with an ID that has
//! a live registered server — no password message. The future hardening step is
//! a confirmation gate: hold the join at `PeerConnected` until the server
//! approves it (new Approve/Deny variants), then send `JoinAccepted`.

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
    let (inbox_tx, mut inbox_rx) = mpsc::channel::<String>(128);
    if let Err(e) = state.registry.insert(remote_id.clone(), display_name.clone(), addr, inbox_tx)
    {
        warn!(%addr, remote_id = %display_remote_id(&remote_id), %e, "register: rejected");
        send_error(&mut ws_tx, &e).await;
        return;
    }

    info!(
        remote_id = %display_remote_id(&remote_id),
        display_name = ?display_name,
        %addr,
        "server registered"
    );

    // ── 3. Send Registered ack ───────────────────────────────────────────────
    let ack = match serde_json::to_string(&RelayMsg::Registered { remote_id: remote_id.clone() }) {
        Ok(s) => s,
        Err(_) => return,
    };
    if ws_tx.send(Message::Text(ack)).await.is_err() {
        state.registry.remove(&remote_id);
        return;
    }

    // ── 4. Bidirectional message pump ────────────────────────────────────────
    // Spawn a task that drains inbox_rx → ws_tx (messages from relay/client to server).
    let remote_id_fwd = remote_id.clone();
    let fwd_task = tokio::spawn(async move {
        while let Some(text) = inbox_rx.recv().await {
            if ws_tx.send(Message::Text(text)).await.is_err() {
                debug!(remote_id = %remote_id_fwd, "forward task: ws write failed");
                break;
            }
        }
    });

    // Main task: ws_rx → route to active room's client (server → relay → client).
    while let Some(frame) = ws_rx.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                debug!(
                    remote_id = %display_remote_id(&remote_id),
                    "server msg: {}",
                    head(&text)
                );
                // Forward verbatim to the paired client (if a room exists).
                if !state.rooms.forward_to_client(&remote_id, text.to_string()).await {
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
    fwd_task.abort();
    state.registry.remove(&remote_id);
    state.rooms.remove(&remote_id);
    info!(
        remote_id = %display_remote_id(&remote_id),
        %addr,
        "server disconnected — registry + room cleaned up"
    );
}

// ── Client join ──────────────────────────────────────────────────────────────

pub async fn handle_join(
    ws: WebSocketUpgrade,
    Path(remote_id): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| join_loop(socket, remote_id, addr, state))
}

async fn join_loop(socket: WebSocket, remote_id: String, addr: SocketAddr, state: AppState) {
    let remote_id = normalize_remote_id(&remote_id);
    let (mut ws_tx, mut ws_rx) = socket.split();

    // ── 1. Authenticate purely from the URL path (the Remote ID IS the join) ─
    let (server_inbox_tx, display_name) = match state.registry.lookup(&remote_id) {
        Some(pair) => pair,
        None => {
            warn!(
                %addr,
                remote_id = %display_remote_id(&remote_id),
                "join: no server online with this Remote ID"
            );
            send_deny(&mut ws_tx, "no server online with this Remote ID").await;
            return;
        }
    };

    // ── 2. Create room ───────────────────────────────────────────────────────
    let room_id = Uuid::new_v4().to_string();
    let (client_inbox_tx, mut client_inbox_rx) = mpsc::channel::<String>(128);

    if let Err(e) = state.rooms.create(remote_id.clone(), room_id.clone(), addr, client_inbox_tx) {
        warn!(%addr, remote_id = %display_remote_id(&remote_id), %e, "join: room create failed");
        send_deny(&mut ws_tx, "server already has an active connection").await;
        return;
    }

    info!(
        remote_id = %display_remote_id(&remote_id),
        display_name = ?display_name,
        room_id = %room_id,
        client = %addr,
        "client joined — room created"
    );

    // ── 3. Notify server of incoming peer ────────────────────────────────────
    // (Future confirmation gate: wait here for the server's Approve/Deny before
    // sending JoinAccepted.)
    let peer_msg = match serde_json::to_string(&RelayMsg::PeerConnected {
        room_id: room_id.clone(),
        client_addr: addr.to_string(),
    }) {
        Ok(s) => s,
        Err(_) => {
            state.rooms.remove(&remote_id);
            return;
        }
    };
    if server_inbox_tx.send(peer_msg).await.is_err() {
        warn!(remote_id = %display_remote_id(&remote_id), "join: server inbox closed right after lookup");
        state.rooms.remove(&remote_id);
        send_deny(&mut ws_tx, "server disconnected during handshake").await;
        return;
    }

    // ── 4. Send JoinAccepted to client ───────────────────────────────────────
    let accepted = match serde_json::to_string(&RelayMsg::JoinAccepted {
        remote_id: remote_id.clone(),
        room_id: room_id.clone(),
    }) {
        Ok(s) => s,
        Err(_) => {
            state.rooms.remove(&remote_id);
            return;
        }
    };
    if ws_tx.send(Message::Text(accepted)).await.is_err() {
        state.rooms.remove(&remote_id);
        return;
    }

    // ── 5. Bidirectional message pump ────────────────────────────────────────
    // Spawn a task that drains client_inbox_rx → ws_tx (server → relay → client).
    let remote_id_fwd = remote_id.clone();
    let fwd_task = tokio::spawn(async move {
        while let Some(text) = client_inbox_rx.recv().await {
            if ws_tx.send(Message::Text(text)).await.is_err() {
                debug!(remote_id = %remote_id_fwd, "client fwd task: ws write failed");
                break;
            }
        }
    });

    // Main task: ws_rx → server's inbox (client → relay → server).
    while let Some(frame) = ws_rx.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                debug!(
                    remote_id = %display_remote_id(&remote_id),
                    client = %addr,
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
    fwd_task.abort();
    state.rooms.remove(&remote_id);
    // Tell the server the viewer is gone. Its own teardown hangs off the WebRTC peer
    // state, which never reaches Failed/Closed when ICE never completed in the first
    // place — so without this a timed-out client leaves `session_active` set forever
    // and every later join is refused with "a session is already active".
    // ponytail: synthesized rather than forwarded; a PeerDisconnected variant is the
    // clean version. Ordering is safe — a later PeerConnected rides the same inbox.
    let _ = server_inbox_tx.send(r#"{"type":"session_stop"}"#.to_string()).await;
    info!(
        remote_id = %display_remote_id(&remote_id),
        room_id = %room_id,
        client = %addr,
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
