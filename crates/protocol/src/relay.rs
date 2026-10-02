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
//! The relay itself never inspects post-handshake messages — it is a dumb pipe. The handshake
//! variants here are mirrored by [`crate::relay_wire::WireMsg`], which is all the relay parses.

use serde::{Deserialize, Serialize};

use crate::{SessionConfig, SessionInfo};

pub use crate::relay_wire::{
    display_remote_id, normalize_remote_id, RELAY_JOIN_BASE_PATH, RELAY_REGISTER_PATH, WIRE_VERSION,
};

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
        /// Handshake version the daemon speaks ([`WIRE_VERSION`]); `0` = predates versioning.
        #[serde(default)]
        v: u32,
        /// This daemon's place in its pool (`WADO_INSTANCE`), stable across restarts of the
        /// daemon *and* of the relay. It is what lets a client come back to the same daemon
        /// after the relay restarts. Empty = a daemon from before this field; the relay then
        /// mints a uuid per connection, as it always did.
        #[serde(default)]
        instance_key: String,
        /// Fresh on every daemon start. Same key + same `boot_id` is one daemon redialling;
        /// a different `boot_id` is a different process, which the relay refuses rather than
        /// let it take the seat.
        #[serde(default)]
        boot_id: String,
        /// How long the relay should hold this daemon's seat for a viewer that drops, in ms —
        /// the daemon's own `VIEWER_GRACE`, so the seat and the desktop it leads to expire
        /// together. `0` = do not hold (a daemon from before seats).
        #[serde(default)]
        hold_ms: u64,
        /// Optional daemon behaviours: `"pong"` (answers relay pings, so the relay may time
        /// out a silent daemon) and `"gate"` (decides each join with `peer_accept` /
        /// `peer_reject`, so the relay must wait for it).
        #[serde(default)]
        caps: Vec<String>,
    },

    // ── Relay → server (handshake) ──────────────────────────────────────────
    /// Ack: server successfully registered.
    Registered {
        remote_id: String,
        /// Handshake version the relay speaks; `0` = a relay that predates versioning.
        #[serde(default)]
        relay_v: u32,
        /// Optional relay behaviours this relay offers. Use a feature only when it is listed.
        #[serde(default)]
        caps: Vec<String>,
    },
    /// A client has joined the room and is waiting for WebRTC negotiation.
    /// Future confirmation gate: the relay will hold the join here until the
    /// server replies Approve/Deny (new variants), only then sending JoinAccepted.
    PeerConnected {
        room_id: String,
        client_addr: String,
        /// The browser's own random id (`localStorage`). What the daemon's trust list holds,
        /// and what the relay holds a dropped viewer's seat for. Empty = an older client.
        #[serde(default)]
        client_key: String,
        /// A human label for the device ("Android · Chrome"), for prompts and logs only.
        #[serde(default)]
        client_name: String,
    },
    /// Relay → daemon: *would* you let this device in? Asked before a takeover, so that a device
    /// the daemon does not trust can never displace a live viewer — it must be approved first,
    /// and the viewer it would displace is still connected to approve it. Answered with
    /// `peer_accept` / `peer_reject` like a join; an accepted room is let straight in when its
    /// `peer_connected` follows.
    PeerCheck {
        room_id: String,
        client_addr: String,
        #[serde(default)]
        client_key: String,
        #[serde(default)]
        client_name: String,
    },
    /// Daemon → relay: let the viewer of `room_id` in. Only a daemon that listed `"gate"` in
    /// its caps sends these, and only then does the relay wait for one.
    PeerAccept {
        room_id: String,
    },
    /// Daemon → relay: refuse the viewer of `room_id`; `reason` is shown to it.
    PeerReject {
        room_id: String,
        #[serde(default)]
        reason: String,
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
        /// Which daemon of the Remote ID's pool this client was given.
        ///
        /// A Remote ID names a **pool** of `wado` daemons, each a whole process with its own
        /// compositor, encoder and applications, so that several devices can use one Remote ID
        /// at the same time. This says which one answered. A client that stores it and passes
        /// it back as `?instance=` on a later join returns to *its own* desktop instead of
        /// being handed a fresh one.
        #[serde(default)]
        instance_id: String,
        /// How many daemons are registered under this Remote ID.
        #[serde(default)]
        pool_size: usize,
        /// How many of them already have a client — this one included.
        #[serde(default)]
        pool_busy: usize,
        /// How this client came to be on this instance: `"reclaimed"` (it asked for this one
        /// by id), `"assigned"` (first free daemon in the pool).
        ///
        /// Both branches speak, deliberately — see the refusal reason in [`RelayMsg::JoinDenied`].
        /// A marker that only reports the good case reads the same as nobody looking.
        #[serde(default)]
        assignment: String,
        /// The answering daemon's `boot_id`. A client that sees it change for the same
        /// `instance_id` knows the daemon restarted and the desktop it left is gone.
        #[serde(default)]
        boot_id: String,
        /// Same as in [`RelayMsg::Registered`], for the client.
        #[serde(default)]
        relay_v: u32,
        #[serde(default)]
        caps: Vec<String>,
    },
    /// Join denied (no server online with this Remote ID, room full, …).
    JoinDenied {
        reason: String,
        /// The refusal is "in use by another device", and joining again with `takeover=1`
        /// would move the seat here. A client offers that as a button, never does it itself.
        #[serde(default)]
        takeover: bool,
        /// Rate-limited: try again no sooner than this.
        #[serde(default)]
        retry_ms: u64,
    },
    /// Relay → client: the join is parked, not refused — the socket stays open and
    /// `join_accepted` follows on it when the reason clears (a daemon registers, the client's
    /// own daemon comes back, a device approves this one). `ms_left` is how long the relay
    /// will keep waiting where that is known, else 0.
    Waiting {
        reason: String,
        #[serde(default)]
        ms_left: u64,
    },
    /// Relay → client: another device took this seat with a deliberate tap. The client must
    /// **not** reconnect on its own — that is what keeps two devices from trading a seat
    /// forever (I17); it offers its own "use it here" instead.
    TakenOver {
        #[serde(default)]
        by: String,
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
    /// Change the shape of the **running** session — resolution, aspect ratio, frame rate,
    /// bitrate — without stopping it.
    ///
    /// The alternative was `SessionStop` + `SessionStart`, and that kills every application the
    /// session launched: "change the bitrate" meant "lose your browser", which is why nobody
    /// could do it while connected. Only the encoder, the capture target and the `Output` depend
    /// on these numbers; the desktop does not, and is left alone.
    ///
    /// Answered with [`RelayMsg::SessionReconfigured`] rather than
    /// [`RelayMsg::SessionStarted`] — deliberately a different message, because
    /// `SessionStarted` is what tells a client to negotiate WebRTC, and a reconfigure must
    /// **not** renegotiate. The track is the same one; only the stream's SPS changes, which a
    /// decoder handles from the forced IDR. Renegotiating would cost a fresh ICE round and a
    /// black screen to change a number.
    SessionReconfigure {
        config: SessionConfig,
    },
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
    /// The running session changed shape. Carries the *new* [`SessionInfo`], so the client can
    /// re-aim its health verdict: the decode budget is `1000 / fps` and the arrival comparison
    /// is against the CBR target, and both just moved. See [`RelayMsg::SessionReconfigure`].
    SessionReconfigured {
        info: SessionInfo,
    },
    /// A launch command was accepted.
    SessionLaunched,
    /// A window action was accepted.
    SessionWindowed,
    /// Which actionable elements lie within `r` of the point — asked on touch-down so the
    /// answer is usually back before the finger lifts. `x`, `y` normalized to the output; `r`
    /// as a fraction of the output's width. `seq` pairs the answer with the question.
    TargetsRequest {
        seq: u32,
        x: f64,
        y: f64,
        r: f64,
    },
    /// The answer to [`RelayMsg::TargetsRequest`]. `None`: the app under the point exposes no
    /// accessibility tree (or there was no window), so the client must fall back to looking at
    /// pixels. `Some(vec![])`: it does, and nothing actionable is near.
    Targets {
        seq: u32,
        targets: Option<Vec<crate::Target>>,
    },
    /// The menu open on the focused window, or `None` once it has closed — state, sent on
    /// every change. See [`crate::MenuSheet`].
    Menu {
        menu: Option<crate::MenuSheet>,
    },
    /// Activate a row of the menu sheet (client → server): [`crate::MenuItem::id`].
    MenuActivate {
        id: String,
    },
    /// The session's windows, in strip order. Sent whenever any window's id, title, app_id or
    /// focus changes, and once when a viewer attaches — see [`crate::WindowInfo`].
    Windows {
        windows: Vec<crate::WindowInfo>,
    },
    /// The launchable applications the server found.
    AppsList {
        apps: Vec<crate::AppEntry>,
    },
    /// A session operation failed.
    SessionError {
        message: String,
    },

    // ── Interactive shells (PTY), several per daemon ───────────────────────
    //
    // Shells belong to the daemon, not the connection: a dropped link leaves them running and
    // the next viewer gets them back, scrollback and all. Every message names its shell by `id`;
    // `id` defaults to 0, which is never a real shell, so an old client's messages are ignored
    // rather than sent to the wrong terminal.
    /// Start a shell sized `cols`x`rows`: a login shell, or `ssh <host>` for an alias from the
    /// host's own ssh config. Answered with [`RelayMsg::PtyOpened`].
    PtyOpen {
        cols: u16,
        rows: u16,
        #[serde(default)]
        host: Option<String>,
    },
    /// The shell [`RelayMsg::PtyOpen`] asked for exists.
    PtyOpened {
        id: u32,
    },
    /// Keystrokes for a shell, exactly as typed — control characters included.
    PtyInput {
        #[serde(default)]
        id: u32,
        data: String,
    },
    /// Output from a shell. UTF-8 text: a read that ends mid-character is held back until the
    /// rest arrives (see `server::pty`). `replay` marks a reattach's scrollback, which replaces
    /// whatever the terminal showed rather than adding to it.
    ///
    /// ponytail: base64 is the upgrade path if byte-exact non-UTF-8 output ever matters.
    PtyOutput {
        #[serde(default)]
        id: u32,
        data: String,
        #[serde(default)]
        replay: bool,
    },
    /// A terminal was resized.
    PtyResize {
        #[serde(default)]
        id: u32,
        cols: u16,
        rows: u16,
    },
    /// Close a shell and everything running under it.
    PtyClose {
        #[serde(default)]
        id: u32,
    },
    /// A shell exited. Its tab stays, with its output, until closed.
    PtyExit {
        #[serde(default)]
        id: u32,
        code: Option<i32>,
    },
    /// Client → server: which shells are open, and the ssh hosts on offer. Answered with
    /// [`RelayMsg::Shells`] and a replay of each shell's scrollback.
    ShellsRequest,
    /// The shells, in the order they were opened, and the ssh aliases a new one can use.
    Shells {
        shells: Vec<crate::ShellInfo>,
        hosts: Vec<String>,
    },

    // ── Running sessions, across the pool ────────────────────────────────────
    /// Client → server: which sessions are running on this computer, on any daemon of the pool.
    SessionsRequest,
    /// Server → client: the answer — one per running session.
    Sessions {
        sessions: Vec<crate::SessionSummary>,
    },
    /// Client → server: this viewer is leaving and the session is to stay running until someone
    /// ends it — not reaped by the no-viewer watchdog.
    SessionDetach,

    // ── The computer itself: sound, Wi-Fi, Bluetooth (see `server::host`) ────
    /// Client → server: send me [`RelayMsg::HostState`]. Needs no session.
    HostGet,
    /// Server → client: the computer as it is now. Sent on request and after every action.
    HostState {
        state: crate::HostState,
    },
    /// Client → server: do this to the computer.
    HostDo {
        action: crate::HostAction,
    },
    /// Server → client: an action failed, and why.
    HostError {
        message: String,
    },

    // ── Notifications from the session's apps ──────────────────────────────
    /// Server → client: an app in the session raised a notification. Same `id` again replaces it.
    Notification {
        id: u32,
        app: String,
        summary: String,
        body: String,
    },
    /// Server → client: the app withdrew it.
    NotificationClosed {
        id: u32,
    },

    // ── Clipboard, as text ─────────────────────────────────────────────────
    /// Client → server: make this the session's clipboard.
    ClipboardSet {
        text: String,
    },
    /// Server → client: an app in the session copied this.
    Clipboard {
        text: String,
    },

    // ── Daemon config (see `wado-config`) ──────────────────────────────────
    /// Client → server: send me [`RelayMsg::ConfigState`]. Needs no session.
    ConfigGet,
    /// Server → client: the config as this viewer sees it. Sent on request, and again to the
    /// viewer whenever the file reloads.
    ConfigState {
        state: crate::ConfigState,
    },
    /// Client → server: set one config key (`stream.max-fps`) in `ui.kdl`. `value` is text read
    /// as KDL would read it — `90`, `true`, `wss://…` — and empty unsets the key. Privileged
    /// keys need the owner device and `confirmed` (an on-screen yes).
    ///
    /// ponytail: a string rather than a typed value, because the schema already types it on load
    /// and the write is checked by exactly that load.
    ConfigSet {
        key: String,
        value: String,
        #[serde(default)]
        confirmed: bool,
    },
    /// Client → server: save this device's settings blob in `ui.kdl`.
    ConfigSetPrefs {
        prefs: String,
    },
    /// Server → client: a `ConfigSet` was refused, and why.
    ConfigRejected {
        key: String,
        message: String,
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
    /// Whether anyone is actually looking at the page — client → server.
    ///
    /// A backgrounded tab or a locked screen still holds a live peer connection and still
    /// receives RTP; the browser simply stops pulling frames and discards them. Measured
    /// 2026-09-13 12:34:57: **11.9 Mbps leaving the daemon, 65 kbps reaching the decoder**, with
    /// the page hidden. That is the user's mobile data and this machine's encoder spent on
    /// something nobody can see.
    ///
    /// The server cannot observe it — a hidden page is indistinguishable from a watched one at
    /// the transport layer — so, like [`RelayMsg::ViewerStrain`], it has to be told.
    ///
    /// **State, not an event.** Sent on change and once on attach, so a viewer that connects
    /// while hidden is not rendered for either.
    ViewerVisible {
        visible: bool,
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

    // ── Device approval: daemon ↔ an already-connected client (relay forwards) ──
    /// A device the daemon does not trust is waiting to join (it may be waiting on another
    /// daemon of the same pool — they share one trust list). Shown to a connected viewer.
    ApproveRequest {
        id: String,
        name: String,
        addr: String,
    },
    /// The viewer's answer: `verdict` is `"once"`, `"always"` or `"deny"`.
    ApproveAnswer {
        id: String,
        verdict: String,
    },
    /// The request was answered (here or elsewhere) or withdrawn; drop the prompt.
    ApproveCleared {
        id: String,
    },

    // ── Keepalive ────────────────────────────────────────────────────────────
    Ping,
    Pong,

    // ── Generic error ────────────────────────────────────────────────────────
    Error {
        message: String,
    },
}

/// The JS client builds and matches these `type` strings by hand, so a renamed variant
/// breaks the relay path silently. Pin the names the client depends on.
#[cfg(test)]
mod wire_tests {
    use super::RelayMsg;

    #[test]
    fn unit_requests_parse_from_bare_type() {
        for name in [
            "timing_request",
            "apps_request",
            "session_stop",
            "session_rejoin",
            "pty_close",
        ] {
            let json = format!(r#"{{"type":"{name}"}}"#);
            assert!(
                serde_json::from_str::<RelayMsg>(&json).is_ok(),
                "{name} no longer parses"
            );
        }
    }

    #[test]
    fn window_list_and_focus_wire_shape() {
        let json = serde_json::to_value(RelayMsg::Windows {
            windows: vec![crate::WindowInfo {
                id: 7,
                title: "Files".into(),
                app_id: "org.gnome.Nautilus".into(),
                focused: true,
            }],
        })
        .unwrap();
        assert_eq!(json["type"], "windows");
        assert_eq!(json["windows"][0]["app_id"], "org.gnome.Nautilus");
        // What js/relay.js sends when a bar icon is tapped.
        let tap: RelayMsg =
            serde_json::from_str(r#"{"type":"session_window","action":{"focus":{"id":7}}}"#)
                .unwrap();
        assert!(matches!(
            tap,
            RelayMsg::SessionWindow {
                action: crate::WindowAction::Focus { id: 7 }
            }
        ));
    }

    #[test]
    fn precision_messages_parse_as_the_client_sends_them() {
        // js/targets.js and js/menu_sheet.js build these by hand.
        let ask: RelayMsg =
            serde_json::from_str(r#"{"type":"targets_request","seq":3,"x":0.5,"y":0.25,"r":0.06}"#)
                .unwrap();
        assert!(matches!(ask, RelayMsg::TargetsRequest { seq: 3, .. }));
        let pick: RelayMsg =
            serde_json::from_str(r#"{"type":"menu_activate","id":":1.2/org/x/1"}"#).unwrap();
        assert!(matches!(pick, RelayMsg::MenuActivate { id } if id == ":1.2/org/x/1"));
        // The server's "no tree" answer must reach the client as null, not as a missing key.
        let none = serde_json::to_value(RelayMsg::Targets {
            seq: 1,
            targets: None,
        })
        .unwrap();
        assert!(none["targets"].is_null() && none.get("targets").is_some());
    }

    #[test]
    fn timing_round_trips_with_queue_ms() {
        let mut timings = crate::StageTimings::default();
        timings.queue_ms = 1.5;
        let json = serde_json::to_value(RelayMsg::Timing { timings }).unwrap();
        assert_eq!(json["type"], "timing");
        assert_eq!(json["timings"]["queue_ms"], 1.5);
        match serde_json::from_value(json).unwrap() {
            RelayMsg::Timing { timings } => assert_eq!(timings.queue_ms, 1.5),
            other => panic!("round-trip gave {other:?}"),
        }
    }
}
