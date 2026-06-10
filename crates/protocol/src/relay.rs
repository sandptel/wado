//! Relay wire types — shared between `wado-relay` (the broker binary), `wado`
//! (the server's relay client), and `wado-client` (the browser app).
//!
//! All relay communication happens over WebSocket text frames, each carrying a
//! JSON-serialised [`RelayMsg`]. The `type` field is the serde discriminant
//! (`#[serde(tag = "type", rename_all = "snake_case")]`).
//!
//! ## Identity: the Remote ID
//! A server is identified by a single **Remote ID** — a 9-digit numeric token
//! (displayed as `528-491-307`; separators are stripped before use). The Remote ID
//! is both the *address* and the *access token*: a client that knows it may connect.
//! A user-confirmation gate (relay asks the server to approve each incoming peer)
//! is the planned hardening step and slots in between `PeerConnected` and
//! `JoinAccepted` without changing this wire format.
//!
//! ## Handshake flow (server side)
//! 1. Server opens WS at `ws://<relay>/register`.
//! 2. Server → relay: `Register { remote_id, display_name }`.
//! 3. Relay → server: `Registered { remote_id }`.
//! 4. Connection stays open; server waits for forwarded client messages.
//! 5. When a client joins: relay → server: `PeerConnected { room_id, client_addr }`.
//!
//! ## Handshake flow (client side)
//! 1. Client opens WS at `ws://<relay>/join/:remote_id` — the path IS the join;
//!    there is no separate join message.
//! 2. Relay → client: `JoinAccepted { ... }` or `JoinDenied { reason }`.
//!
//! ## After handshake
//! All subsequent messages are forwarded verbatim by the relay:
//! - Client → relay → server: session control + SDP offer + ICE candidates.
//! - Server → relay → client: session responses + SDP answer + log lines.
//!
//! The relay itself never inspects post-handshake messages — it is a dumb pipe.

use serde::{Deserialize, Serialize};

use crate::{SessionConfig, SessionInfo};

/// WebSocket endpoint the **server** connects to in order to register itself.
/// Path: `ws://<relay>/register`
pub const RELAY_REGISTER_PATH: &str = "/register";

/// WebSocket endpoint the **client** connects to in order to join a server.
/// Path: `ws://<relay>/join/:remote_id`
pub const RELAY_JOIN_BASE_PATH: &str = "/join";

/// Canonicalize a Remote ID: strip the display separators (`-`, spaces) so
/// `528-491-307`, `528 491 307`, and `528491307` all compare equal. Both the
/// relay (register + join) and the server apply this before any comparison.
pub fn normalize_remote_id(id: &str) -> String {
    id.chars().filter(|c| !c.is_whitespace() && *c != '-').collect()
}

/// Human-readable form of a Remote ID: 9 digits grouped as `XXX-XXX-XXX`.
/// Non-9-digit IDs (e.g. a custom `WADO_REMOTE_ID`) are returned unchanged.
pub fn display_remote_id(id: &str) -> String {
    if id.len() == 9 && id.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &id[0..3], &id[3..6], &id[6..9])
    } else {
        id.to_string()
    }
}

/// Every relay WebSocket frame is a JSON-serialised `RelayMsg`.
///
/// Handshake messages are consumed/emitted by the relay itself. All other
/// messages are forwarded verbatim between the paired server and client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayMsg {
    // ── Server → relay (handshake) ──────────────────────────────────────────
    /// Server registers itself under its Remote ID (normalized: digits only).
    Register {
        remote_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
    },

    // ── Relay → server (handshake) ──────────────────────────────────────────
    /// Ack: server successfully registered.
    Registered { remote_id: String },
    /// A client has joined the room and is waiting for WebRTC negotiation.
    /// Future confirmation gate: the relay will hold the join here until the
    /// server replies Approve/Deny (new variants), only then sending JoinAccepted.
    PeerConnected { room_id: String, client_addr: String },

    // ── Relay → client (handshake) ──────────────────────────────────────────
    /// Join accepted; room is open. (The client never sends a join message —
    /// connecting to `/join/:remote_id` with a valid Remote ID is the join.)
    JoinAccepted { remote_id: String, room_id: String },
    /// Join denied (no server online with this Remote ID, room full, …).
    JoinDenied { reason: String },

    // ── Session control: client → server (forwarded by relay) ───────────────
    /// Ask the server to start a compositor session with the given config.
    SessionStart { config: SessionConfig },
    /// Ask the server to stop the running session.
    SessionStop,
    /// Spawn a command into the running session.
    SessionLaunch { command: String },

    // ── Session control: server → client (forwarded by relay) ───────────────
    /// Session started OK; carries encoder/pipeline info.
    SessionStarted { info: SessionInfo },
    /// Session stopped cleanly.
    SessionStopped,
    /// A launch command was accepted.
    SessionLaunched,
    /// A session operation failed.
    SessionError { message: String },

    // ── Live logs: server → client (forwarded by relay) ─────────────────────
    /// One tracing log line in `LEVEL|HH:MM:SS|text` format.
    Log { line: String },

    // ── WebRTC signaling (bidirectional, forwarded by relay) ─────────────────
    /// Client's SDP offer JSON (with all ICE candidates gathered, non-trickle).
    SdpOffer { sdp: String },
    /// Server's SDP answer JSON (with all ICE candidates gathered, non-trickle).
    SdpAnswer { sdp: String },
    /// A single Trickle-ICE candidate (for future trickle ICE support).
    IceCandidate { candidate: String },

    // ── Keepalive ────────────────────────────────────────────────────────────
    Ping,
    Pong,

    // ── Generic error ────────────────────────────────────────────────────────
    Error { message: String },
}
