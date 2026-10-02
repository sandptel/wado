//! Relay signaling: daemons register (`/register`), clients join (`/join/:remote_id`), and
//! after the handshake both sides enter a pure-forwarding loop — the relay never inspects
//! post-handshake message content beyond the few frames addressed to the relay itself
//! (`pong`, `peer_accept`, `peer_reject`). See `wado_protocol::relay_wire` for the frozen
//! handshake and `WADO_PLAN.md`, Decision Log `2026-10-02`, for why it is frozen.
//!
//! Auth model: the **Remote ID** addresses a pool of daemons, and a daemon that lists `gate`
//! in its caps decides each join itself — the relay holds the join at `peer_connected` until
//! the daemon answers `peer_accept` or `peer_reject`. The policy (trusted devices, who may
//! approve) lives in the daemon, so it can change without touching the relay. A daemon without
//! `gate` is joined on the Remote ID alone, as before.
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
//!
//! ## Where the code lives
//!
//! [`crate::register`] serves daemons, [`crate::join`] serves clients. This file holds what both
//! share: timing, the relay's caps, the pool-full wording and the small socket helpers.

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::SplitSink;
use futures_util::SinkExt;
use wado_protocol::relay_wire::WireMsg;

pub type WsTx = SplitSink<WebSocket, Message>;

/// How often the relay pings each client, and each daemon that answers pings.
///
/// Well under the ~100 s after which a cloudflared quick tunnel drops an idle connection, and
/// a third of [`SILENCE`], so one lost ping never ends a link on its own.
pub const KEEPALIVE: Duration = Duration::from_secs(15);

/// A peer that has sent nothing — not even a pong — for this long is gone. Its socket is
/// closed, which is what lets the seat be held for it and the other side be told; a half-open
/// TCP connection behind a tunnel can otherwise look alive for hours.
pub const SILENCE: Duration = Duration::from_secs(45);

/// Optional behaviours this relay offers, sent in `registered` and `join_accepted`. A peer uses
/// a behaviour only when it is listed here, which is how the relay gains features without
/// breaking daemons and clients older than it. Append; never rename.
///
/// - `park`: a join with no daemon to go to waits (`waiting`) instead of being refused.
/// - `hold`: a dropped client's seat is kept for it for the daemon's hold time.
/// - `takeover`: `takeover=1` on a join moves a seat from another device.
/// - `gate`: joins wait for the daemon's `peer_accept` / `peer_reject`.
/// - `ping`: the relay pings daemons that list `pong`, and closes ones that fall silent.
/// - `leave`: a client closing with `LEAVE_CLOSE_CODE` frees its seat rather than holding it.
pub const CAPS: &[&str] = &["park", "hold", "takeover", "gate", "ping", "leave"];

pub fn caps() -> Vec<String> {
    CAPS.iter().map(|c| c.to_string()).collect()
}

/// The refusal when every daemon in the pool has a client.
///
/// The phrase "already has an active connection" is load-bearing, not prose: older clients
/// match it (`OCCUPIED_RE` in `js/relay_link.js`) to tell "the pool is full" from "no daemon is
/// online" and back off hard instead of knocking every 500 ms. Without it two devices trade the
/// session forever — `issues.md` I17, measured as 18 knocks in 94 s.
/// `scripts/relay-link-check.mjs` reads this file and fails if the wording moves.
pub fn occupied_reason(pool_size: usize, holders: &[String]) -> String {
    let who = holders
        .iter()
        .filter(|h| !h.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let who = if who.is_empty() {
        String::new()
    } else {
        format!(" (in use by {who})")
    };
    format!(
        "every one of the {pool_size} wado session(s) on this Remote ID already has an active \
         connection{who}. Use it here to move one to this device, or start another daemon \
         (WADO_INSTANCES) to raise the limit."
    )
}

/// A client's own pong, recognised exactly.
///
/// It was `text.contains("\"pong\"")`, which also swallowed any frame that merely *mentioned*
/// pong as a value — `{"type":"session_launch","command":"pong"}` never reached the daemon.
pub fn is_pong(text: &str) -> bool {
    text.trim() == r#"{"type":"pong"}"#
}

pub async fn send_wire(ws_tx: &mut WsTx, msg: &WireMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(text) => ws_tx.send(Message::Text(text)).await.is_ok(),
        Err(_) => false,
    }
}

pub async fn send_error(ws_tx: &mut WsTx, message: &str) {
    send_wire(
        ws_tx,
        &WireMsg::Error {
            message: message.to_string(),
        },
    )
    .await;
}

pub async fn send_deny(ws_tx: &mut WsTx, reason: &str, takeover: bool, retry_ms: u64) {
    send_wire(
        ws_tx,
        &WireMsg::JoinDenied {
            reason: reason.to_string(),
            takeover,
            retry_ms,
        },
    )
    .await;
}

/// The device's own address, as opposed to the socket the relay sees.
///
/// Every client that arrives through the cloudflared tunnel connects from `127.0.0.1`, so the
/// socket address answers "did it come through the tunnel", never "who is it". Cloudflare puts
/// the real one in `CF-Connecting-IP`; a plain reverse proxy uses `X-Forwarded-For`, whose first
/// entry is the originating client. Direct LAN clients have neither and the socket is the truth.
///
/// The headers are client-settable, so they are believed only with `--trust-proxy` — set it
/// when, and only when, every connection arrives through a proxy that overwrites them. The join
/// rate limit keys on this, so believing a forged header would let one machine look like many.
pub fn peer_ip(headers: &axum::http::HeaderMap, addr: SocketAddr, trust_proxy: bool) -> String {
    let forwarded = || {
        headers
            .get("cf-connecting-ip")
            .or_else(|| headers.get("x-forwarded-for"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    trust_proxy
        .then(forwarded)
        .flatten()
        .unwrap_or_else(|| addr.ip().to_string())
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
pub fn head(text: &str) -> String {
    const LOG_HEAD: usize = 120;
    text.chars().take(LOG_HEAD).collect()
}

#[cfg(test)]
mod tests {
    use super::{head, is_pong};

    #[test]
    fn only_a_real_pong_is_swallowed() {
        assert!(is_pong(r#"{"type":"pong"}"#));
        // Frames that merely mention pong must reach the daemon.
        assert!(!is_pong(r#"{"type":"session_launch","command":"pong"}"#));
        assert!(!is_pong(r#"{"type":"pty_input","data":"\"pong\""}"#));
    }

    #[test]
    fn head_never_splits_a_codepoint() {
        // The exact shape that panicked: 119 ASCII bytes, then a 3-byte char occupying bytes
        // 119..122 — so the old `&text[..120]` cut straight through it.
        let s = format!("{}●tail", "a".repeat(119));
        assert!(
            !s.is_char_boundary(120),
            "test no longer reproduces the original panic"
        );
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
pub struct AbortOnDrop(pub tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
