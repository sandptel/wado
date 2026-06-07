//! Wire types shared between the wado **server** (the compositor + control plane)
//! and its **clients** (currently the Dioxus web app). Keeping these in one crate
//! stops the client's serialized requests and the server's deserialization from
//! drifting apart.
//!
//! This crate is deliberately dependency-light (just `serde`) so it compiles for
//! both the host (server) and the `wasm32` (web client) targets.

use serde::{Deserialize, Serialize};

/// HTTP endpoints the client talks to on the server. Shared as constants so the
/// two sides cannot disagree on a path.
pub mod endpoints {
    /// `POST` a [`crate::SessionConfig`] (JSON) to start a session.
    pub const SESSION_START: &str = "/session/start";
    /// `POST` (empty) to tear the active session down.
    pub const SESSION_STOP: &str = "/session/stop";
    /// `POST` a JSON-encoded command string to spawn into the *running* session
    /// (in addition to any started ones). Lets the client launch apps in realtime.
    pub const SESSION_LAUNCH: &str = "/session/launch";
    /// `POST` a WebRTC SDP offer (JSON); the answer comes back as JSON.
    pub const OFFER: &str = "/offer";
    /// `GET` the live tracing log stream as Server-Sent Events.
    pub const EVENTS: &str = "/events";
}

/// Label of the WebRTC **data channel** the client opens to carry input
/// ([`InputEvent`]s). Shared so the client (which creates it) and the server (which
/// matches it in `on_data_channel`) cannot disagree.
pub const INPUT_CHANNEL: &str = "wado-input";

/// One input event from the remote client, sent as JSON over the input data channel.
///
/// All coordinates are **normalized 0..1** relative to the *displayed video content*
/// rect (the client does the letterbox math); the compositor scales them to the output.
///
/// wado renders **no on-screen cursor**. `Touch`/`Key` map straight to `wl_touch`/
/// `wl_keyboard`. `Scroll`/`Button` are delivered via `wl_pointer` (the only Wayland
/// mechanism for axis/secondary-click) by focusing the surface under the point — still
/// without drawing a cursor. `WindowDrag` is a compositor-managed window move (it never
/// reaches the app). See `INPUT_CHALLENGES.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum InputEvent {
    /// A touch contact. `id` identifies the contact (multi-touch: several may be live at
    /// once). `phase` is the lifecycle; `x`/`y` are normalized 0..1.
    Touch {
        id: u32,
        phase: TouchPhase,
        x: f64,
        y: f64,
    },
    /// A key press/release. `code` is the **Linux evdev keycode** (e.g. `KEY_A` = 30),
    /// *before* the xkb +8 offset (the compositor applies it).
    Key { code: u32, pressed: bool },
    /// Absolute pointer motion / hover at (`x`,`y`) — sent for a real mouse (`pointerType
    /// == "mouse"`), so apps get `wl_pointer` motion (hover, menus, tooltips). No cursor is
    /// drawn. Touchscreens use `Touch` instead.
    PointerMotion { x: f64, y: f64 },
    /// A scroll/wheel tick at (`x`,`y`). `dx`/`dy` are already-normalized **pixel** deltas
    /// (the client folds in `deltaMode`, scroll-speed and natural-direction); the compositor
    /// turns them into a value-only `wl_pointer` axis frame.
    Scroll { x: f64, y: f64, dx: f64, dy: f64 },
    /// A pointer button press/release at (`x`,`y`). For a real mouse this is the actual
    /// button; for touch it is the long-press → right-click emulation. Delivered via
    /// `wl_pointer` with focus set to the surface under the point.
    Button {
        x: f64,
        y: f64,
        button: PointerButton,
        pressed: bool,
    },
    /// A compositor-managed window move (long-press-drag or the client's "move mode"). The
    /// window under the `Down` point follows subsequent `Motion`s until `Up`. Handled
    /// entirely by the compositor; never forwarded to the application.
    WindowDrag { phase: TouchPhase, x: f64, y: f64 },
    /// Retract an in-progress touch contact when a gesture takes over (e.g. a long-press
    /// promotes to a window move/right-click), so the app sees a cancel, not a tap. Maps
    /// to `wl_touch`'s **global** cancel (all live contacts), per the protocol.
    CancelTouch { id: u32 },
}

/// Which pointer button a [`InputEvent::Button`] refers to. The compositor maps these to the
/// Linux `BTN_*` codes (`Left`=`0x110`, `Middle`=`0x112`, `Right`=`0x111`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

/// Lifecycle phase of a touch contact (maps to `wl_touch` down / motion / up).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchPhase {
    Down,
    Motion,
    Up,
}

/// Image-quality preset chosen by the client (RustDesk's model). The server maps
/// this to a concrete bitrate / x264 preset / keyframe interval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Lowest latency: low bitrate, fastest preset, short GOP.
    Reactivity,
    /// Middle ground (default).
    Balanced,
    /// Higher bitrate / better image at some CPU cost.
    Quality,
    /// Explicit CBR target in kbps.
    Custom { bitrate_kbps: u32 },
}

/// One session's configuration: built by the client and sent to the server on
/// `POST /session/start`. The encoder/video fields are flat (the original, stable set);
/// newer behaviour settings are grouped into atomic sub-structs ([`InputConfig`],
/// [`WindowConfig`]) so each settings domain stays independently testable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub quality: Quality,
    /// Advanced override: x264 preset name ("ultrafast".."veryfast"). Falls back to
    /// the quality preset's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Advanced override: frames between IDR keyframes. Falls back to the quality
    /// preset's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyframe_interval: Option<u32>,
    /// Input behaviour (keyboard repeat, focus policy). Applied at session start.
    #[serde(default)]
    pub input: InputConfig,
    /// Window-management behaviour (new-window placement). Applied at session start.
    #[serde(default)]
    pub window: WindowConfig,
}

/// Input-behaviour settings for a session (the "Compositor settings → input" group).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InputConfig {
    /// xkb key-repeat rate in keys/second.
    pub repeat_rate: i32,
    /// xkb key-repeat delay in milliseconds before repeat begins.
    pub repeat_delay: i32,
    /// When true, hovering a window with the pointer also gives it keyboard focus.
    pub focus_follows_pointer: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        // Mirrors the historical `seat.add_keyboard(_, 200, 25)` defaults.
        Self { repeat_rate: 25, repeat_delay: 200, focus_follows_pointer: false }
    }
}

/// Window-management settings for a session (the "Compositor settings → window" group).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Where newly-mapped toplevels are placed on the output.
    #[serde(default)]
    pub placement: Placement,
}

/// New-window placement policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// Map at the output origin (0,0) — the historical behaviour.
    TopLeft,
    /// Centre the window on the output.
    #[default]
    Center,
    /// Step each new window down-right from the last (cascade).
    Cascade,
    /// Size the window to the output and map at (0,0).
    Maximized,
}

/// Live-log wire format shared by the server's log bus (which formats lines) and
/// the client's log panel (which parses them).
///
/// Format: `LEVEL|HH:MM:SS|text` — two `|` separators, `text` may itself contain
/// `|`, so callers split on the *first two* only.
pub mod logfmt {
    /// Separator between the three leading fields.
    pub const DELIM: char = '|';

    /// A parsed log line.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LogLine {
        pub level: String,
        pub ts: String,
        pub text: String,
    }

    /// Format the three fields into one wire line. `text` is free-form (it may
    /// contain `|`); the parser only splits on the first two separators.
    pub fn format_line(level: &str, ts: &str, text: &str) -> String {
        format!("{level}{DELIM}{ts}{DELIM}{text}")
    }

    /// Parse a wire line back into its fields. A line missing the separators is
    /// treated as all-`text` at the default `INFO` level so nothing is dropped.
    pub fn parse(line: &str) -> LogLine {
        match line.split_once(DELIM) {
            Some((level, rest)) => match rest.split_once(DELIM) {
                Some((ts, text)) => LogLine {
                    level: level.to_string(),
                    ts: ts.to_string(),
                    text: text.to_string(),
                },
                None => LogLine {
                    level: "INFO".to_string(),
                    ts: String::new(),
                    text: line.to_string(),
                },
            },
            None => LogLine {
                level: "INFO".to_string(),
                ts: String::new(),
                text: line.to_string(),
            },
        }
    }
}
