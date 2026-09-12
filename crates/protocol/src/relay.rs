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
    id.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
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
    Registered {
        remote_id: String,
    },
    /// A client has joined the room and is waiting for WebRTC negotiation.
    /// Future confirmation gate: the relay will hold the join here until the
    /// server replies Approve/Deny (new variants), only then sending JoinAccepted.
    PeerConnected {
        room_id: String,
        client_addr: String,
    },
    /// The viewer's WebSocket closed. The room is gone; the **session is not**.
    ///
    /// This replaces a synthesized `{"type":"session_stop"}` the relay used to send here. That
    /// was written when the only teardown was the WebRTC peer state reaching `Failed`/`Closed`,
    /// which never happens when ICE never completed — so a timed-out client left `session_active`
    /// set forever. `viewer_watchdog` covers that case now, by two independent clocks.
    ///
    /// The old line meanwhile converted **any** socket close — a cell handoff, a screen lock, a
    /// tunnel hiccup — into an instant full teardown, taking the windows and every launched
    /// application with it and giving the grace period nothing to grace. A viewer going away is
    /// not a request to stop; it is the absence of a request.
    PeerDisconnected {
        room_id: String,
    },

    // ── Relay → client (handshake) ──────────────────────────────────────────
    /// Join accepted; room is open. (The client never sends a join message —
    /// connecting to `/join/:remote_id` with a valid Remote ID is the join.)
    JoinAccepted {
        remote_id: String,
        room_id: String,
    },
    /// Join denied (no server online with this Remote ID, room full, …).
    JoinDenied {
        reason: String,
    },

    // ── Session control: client → server (forwarded by relay) ───────────────
    /// Ask the server to start a compositor session with the given config.
    SessionStart {
        config: SessionConfig,
    },
    /// Attach to the session that is *already* running, instead of starting a new one.
    ///
    /// The answer to [`RelayMsg::SessionAlive`]. Nothing is torn down: the compositor keeps its
    /// windows, its applications and their state, and the only thing that happens is a forced
    /// keyframe so the new viewer's decoder has something to start from. The reply is an
    /// ordinary [`RelayMsg::SessionStarted`], so the client's negotiation path is the same one
    /// a fresh session takes.
    SessionRejoin,
    /// Ask the server to stop the running session.
    SessionStop,
    /// Spawn a command into the running session.
    SessionLaunch {
        command: String,
    },
    /// Ask for the list of launchable applications.
    ///
    /// Relay mode has no HTTP path to the server, so the `GET /apps` route needs a message
    /// counterpart. Unlike the session verbs this needs no running session — you pick what to
    /// launch before there is anything to launch it into.
    AppsRequest,
    /// Act on the running session's focused window.
    ///
    /// A peer variant rather than a nesting inside `SessionLaunch`: this enum is flat and
    /// forwarded verbatim, so one variant per action is the idiom here and costs the relay
    /// nothing. (HTTP consolidates instead — see [`crate::SessionControl`].)
    SessionWindow {
        action: crate::WindowAction,
    },

    // ── Session control: server → client (forwarded by relay) ───────────────
    /// Session started OK; carries encoder/pipeline info.
    SessionStarted {
        info: SessionInfo,
    },
    /// A session was already running when the client asked to start one.
    ///
    /// Sent **instead of** [`RelayMsg::SessionStarted`], and instead of the
    /// [`RelayMsg::SessionError`] this used to be. A reconnecting viewer — or a second
    /// device — would otherwise be told "a session is already active" and left with nothing to
    /// do about it, while the session it could have joined kept running behind the error.
    ///
    /// The client answers with [`RelayMsg::SessionRejoin`] to attach to it, or
    /// [`RelayMsg::SessionStop`] followed by a fresh [`RelayMsg::SessionStart`] to replace it.
    /// It carries the running session's [`SessionInfo`] so the choice can be an informed one
    /// rather than a blind guess about what is on the other end.
    SessionAlive {
        info: SessionInfo,
    },
    /// Session stopped cleanly.
    SessionStopped,
    /// A launch command was accepted.
    SessionLaunched,
    /// A window action was accepted.
    SessionWindowed,
    /// The launchable applications the server found.
    AppsList {
        apps: Vec<crate::AppEntry>,
    },
    /// A session operation failed.
    SessionError {
        message: String,
    },

    // ── Interactive shell (PTY) ─────────────────────────────────────────────
    /// Start a login shell on a pseudo-terminal, sized `cols`x`rows`.
    ///
    /// A PTY is what makes a shell behave like a shell: job control, line editing, colour,
    /// and full-screen programs all key off being attached to a terminal. Opening twice
    /// replaces the first — one shell per viewer.
    PtyOpen {
        cols: u16,
        rows: u16,
    },
    /// Keystrokes for the shell, exactly as typed — control characters included.
    ///
    /// Always valid UTF-8: this is what the terminal emulator produced from a key event, and
    /// a control byte like `0x03` is a perfectly good `char`.
    PtyInput {
        data: String,
    },
    /// Output from the shell, for the terminal emulator to interpret.
    ///
    /// UTF-8 text rather than bytes, which costs one thing and buys another. A PTY read can
    /// end mid-character, so the server holds the incomplete tail back until the rest
    /// arrives (see `server::pty`). Output that is not UTF-8 at all — a stray `cat` of a
    /// binary — arrives as replacement characters, which is what a terminal shows anyway.
    ///
    /// ponytail: base64 is the upgrade path if byte-exact non-UTF-8 output ever matters.
    PtyOutput {
        data: String,
    },
    /// The terminal was resized. Full-screen programs redraw from this, and a shell that
    /// never receives it wraps its lines at the wrong column.
    PtyResize {
        cols: u16,
        rows: u16,
    },
    /// Close the shell and everything running under it.
    PtyClose,
    /// The shell exited. `code` is absent when it was killed by a signal.
    PtyExit {
        code: Option<i32>,
    },

    /// Ask for the server's per-stage render timings.
    ///
    /// Relay mode has no HTTP path, so `GET /timing` needs a message counterpart the same way
    /// `GET /apps` did. Without it the latency breakdown silently shows only the browser's
    /// half — an absent capture/encode/queue reading is easy to misread as a fast one.
    TimingRequest,
    /// The server's answer to [`RelayMsg::TimingRequest`].
    Timing {
        timings: crate::StageTimings,
    },

    // ── Live logs: server → client (forwarded by relay) ─────────────────────
    /// One tracing log line in `LEVEL|HH:MM:SS|text` format.
    Log {
        line: String,
    },
    /// The focused application asked for, or gave up, text input — `zwp_text_input_v3`,
    /// server → client.
    ///
    /// This is what lets a phone raise its soft keyboard when a text field is focused instead
    /// of the viewer having to press ⌨ first. It is **state, not an event**: sent on change and
    /// once when a viewer attaches, so a viewer that joins a session mid-edit is told the
    /// keyboard should already be up.
    TextInput {
        active: bool,
    },
    /// Bitrate the server actually wrote to the video track over the last stretch, kbps —
    /// server → client.
    ///
    /// **The discriminator the viewer cannot compute.** It sees what arrived; it does not see what
    /// was sent, so "little is arriving and nothing was lost" is ambiguous between a sender that
    /// stopped and a path that is discarding silently. Measured twice, and the verdict blamed the
    /// wrong party both times:
    ///
    /// | when | sent | arrived | `packetsLost` | strip said |
    /// |---|---|---|---|---|
    /// | 2026-09-12 22:33 | 5.35 Mbps | 2.44 Mbps | 0 | `bad the server` |
    /// | 2026-09-13 01:19 | 5.13 Mbps | 524 kbps | 0 | `bad the server` |
    ///
    /// Render pacing held 90/90 and the pump was clean through both. The lesson is narrower than
    /// "trust the server": **`packetsLost = 0` does not mean no loss**, it means that counter has
    /// nothing to say, and a rule that reads it as good news accuses whoever is left.
    ///
    /// Measured at the track rather than taken from the encoder config, because the question is
    /// what left the process, not what was asked for.
    SentKbps {
        kbps: u32,
    },
    /// How many render ticks in every N the compositor is actually sending — server → client.
    ///
    /// 1 means nothing is being shed. Anything higher is a mitigation the *viewer* asked for (or
    /// the pump did), and the viewer has to be told, because otherwise it measures the effect and
    /// blames the sender: measured 2026-09-12 16:17:33, `bad the server — only 816 kbps arriving
    /// of 5.7 Mbps` about a frame rate the phone had requested three seconds earlier.
    ///
    /// **State, not an event**, like [`RelayMsg::TextInput`]: sent on change and once when a
    /// viewer attaches, so one joining a shedding session is not misled either.
    Shedding {
        divisor: u32,
    },
    /// The viewer's decoder is, or is no longer, saturated — client → server.
    ///
    /// The one congestion signal the server cannot measure for itself. It can see its own pump
    /// back up; it cannot see a phone decoding 15 of the 90 frames a second it is being sent,
    /// which has been measured here with every server-side metric clean for 86 seconds.
    ///
    /// **State, not an event**, like [`RelayMsg::TextInput`]: sent only when the client's
    /// *settled* verdict changes, so between messages the last value stands. The client gates it
    /// on the stream actually arriving — a decoder starved of frames looks identical to an
    /// overloaded one, and shedding for the first makes a network fault worse.
    ViewerStrain {
        strained: bool,
    },
    /// One diagnostic line from the browser, client → server. The phone's console is
    /// unreachable during a field test, so the client ships what it sees — ICE candidate
    /// types above all — to the server, which logs it.
    ClientLog {
        line: String,
    },

    // ── WebRTC signaling (bidirectional, forwarded by relay) ─────────────────
    /// Client's SDP offer JSON (with all ICE candidates gathered, non-trickle).
    SdpOffer {
        sdp: String,
    },
    /// Server's SDP answer JSON (with all ICE candidates gathered, non-trickle).
    SdpAnswer {
        sdp: String,
    },
    /// A single Trickle-ICE candidate (for future trickle ICE support).
    IceCandidate {
        candidate: String,
    },

    // ── Keepalive ────────────────────────────────────────────────────────────
    Ping,
    Pong,

    // ── Generic error ────────────────────────────────────────────────────────
    Error {
        message: String,
    },
}
