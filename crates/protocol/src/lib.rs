//! Wire types shared between the wado **server** (the compositor + control plane)
//! and its **clients** (currently the Dioxus web app). Keeping these in one crate
//! stops the client's serialized requests and the server's deserialization from
//! drifting apart.
//!
//! This crate is deliberately dependency-light (just `serde`) so it compiles for
//! both the host (server) and the `wasm32` (web client) targets.

pub mod apps;
pub mod control;
pub mod relay;

use serde::{Deserialize, Serialize};

/// HTTP endpoints the client talks to on the server. Shared as constants so the
/// two sides cannot disagree on a path.
pub use apps::AppEntry;
pub use control::{SessionControl, WindowAction};

pub mod endpoints {
    /// `POST` a [`crate::SessionConfig`] (JSON) to start a session.
    pub const SESSION_START: &str = "/session/start";
    /// `POST` (empty) to tear the active session down.
    pub const SESSION_STOP: &str = "/session/stop";
    /// `POST` a JSON [`crate::SessionControl`] to act on the *running* session — launch a
    /// command, or act on the focused window. One route for every verb: the previous
    /// one-route-per-verb shape meant a new HTTP handler *and* a new relay message for each.
    pub const SESSION_CONTROL: &str = "/session/control";
    /// `POST` a WebRTC SDP offer (JSON); the answer comes back as JSON.
    pub const OFFER: &str = "/offer";
    /// `GET` the live tracing log stream as Server-Sent Events.
    pub const EVENTS: &str = "/events";
    /// `GET` the installed applications the server can launch, as a JSON array of
    /// [`crate::AppEntry`].
    pub const APPS: &str = "/apps";
}

/// Label of the WebRTC **data channel** the client opens to carry input
/// ([`InputEvent`]s). Shared so the client (which creates it) and the server (which
/// matches it in `on_data_channel`) cannot disagree.
pub const INPUT_CHANNEL: &str = "wado-input";

/// Label of the **second** input data channel, carrying only high-rate *positional*
/// updates (pointer motion, window-drag motion).
///
/// Invariant #1: input must never head-of-line-block. `INPUT_CHANNEL` is reliable and
/// ordered because a dropped button-release or keystroke is unrecoverable — but a 1000 Hz
/// mouse pushing motion down that same channel saturates it, and every later event then
/// queues behind the backlog. Positional updates are *latest-wins*: a lost one is
/// corrected by the next, so they ride a **zero-retransmit** channel instead. Ordered, though:
/// the positions are absolute, so a reordered arrival would replay a stale one over a newer
/// one and the pointer would visibly jump backwards.
///
/// Because the two channels have no ordering relationship, anything sent here must be
/// safe to arrive late or out of order. Both current senders are: the compositor ignores
/// `WindowDrag::Motion` when no move is in progress, and pointer motion is absolute, so a
/// stale one is overwritten by the next. Do NOT move a terminal event (button/key/up) or
/// anything stateful onto this channel.
pub const MOTION_CHANNEL: &str = "wado-motion";

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
    /// Relative pointer motion, in the session's **logical pixels** — a movement, not a
    /// position.
    ///
    /// Sent instead of [`InputEvent::PointerMotion`] while the viewer holds a browser pointer
    /// lock, which is what a 3D game needs: a camera turns by how far the mouse moved, and a
    /// position normalised against the video rect stops changing the moment the pointer reaches
    /// the edge of it, so the camera stops while the real mouse keeps going.
    ///
    /// The compositor forwards it as `zwp_relative_pointer_v1` and *also* moves the absolute
    /// pointer, unless the application holds an active pointer lock — see
    /// `wado_compositor::input::relative`.
    PointerRelative { dx: f64, dy: f64 },
    /// A scroll/wheel tick at (`x`,`y`). `dx`/`dy` are already-normalized **pixel** deltas
    /// (the client folds in `deltaMode`, scroll-speed and natural-direction); the compositor
    /// turns them into a value-only `wl_pointer` axis frame.
    Scroll {
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
        /// What produced the scroll. A finger is not a wheel: clients use the source to pick
        /// smooth kinetic scrolling over notched stepping, and to know that an axis-stop will
        /// follow when the contact lifts.
        #[serde(default)]
        source: ScrollSource,
        /// The contact has lifted; emit the axis-stop that ends a finger scroll. Carries no
        /// delta. Meaningless for a wheel, which has no end.
        #[serde(default)]
        stop: bool,
    },
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
    /// A two-finger pinch/rotate, delivered as `zwp_pointer_gestures_v1` pinch events.
    ///
    /// Rides the same two-contact gesture the client uses for scrolling, so one gesture can
    /// produce both — which is exactly what libinput reports for a touchpad, and what
    /// toolkits expect: translation on the scroll axis, magnification here.
    ///
    /// The two figures are measured differently because the protocol defines them
    /// differently, and mixing them up silently inverts a zoom: `scale` is **absolute**
    /// against the distance at `Down`, `rotation` is the **delta in degrees** since the
    /// previous event.
    Pinch {
        phase: TouchPhase,
        x: f64,
        y: f64,
        scale: f64,
        rotation: f64,
    },
    /// Retract an in-progress touch contact when a gesture takes over (e.g. a long-press
    /// promotes to a window move/right-click), so the app sees a cancel, not a tap. Maps
    /// to `wl_touch`'s **global** cancel (all live contacts), per the protocol.
    CancelTouch { id: u32 },
    /// Latency probe. The server echoes `{"t":"pong","seq":…}` straight back on the same
    /// data channel and does **not** forward this to the compositor, so the client can
    /// time the input leg (client → server → client) without a synchronised clock.
    ///
    /// It deliberately rides the reliable channel: it is measuring the path that real
    /// button and key events take, and a probe that could be silently dropped would
    /// measure nothing.
    Ping { seq: u32 },
}

/// Per-stage timings for the server half of the pipeline, averaged over a short window.
///
/// Deliberately NOT a single glass-to-glass figure. The browser and the server have no
/// common clock, so any fused end-to-end number would be guesswork; these are the legs
/// that are honestly measurable on the server, and the client measures its own legs
/// (network, playout buffer, decode) from `getStats()`. Reported in milliseconds.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StageTimings {
    /// Render the scene into the capture target (GL draw + DMA-BUF export / CPU readback).
    pub capture_ms: f64,
    /// Hand the captured frame to the encoder and get an access unit back.
    pub encode_ms: f64,
    /// How long the encoded frame then waited to be accepted by the WebRTC pump. A
    /// non-zero value here means the network is the bottleneck, not the GPU.
    pub queue_ms: f64,
    /// Interval between render ticks, which is the frame pacing actually achieved.
    pub tick_ms: f64,
    /// Achieved frames per second over the window.
    pub fps: f64,
    /// Frames the pump refused since the session started (stale-frame indicator).
    pub dropped: u64,
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
    /// Output scale factor, as Hyprland's `monitor=...,scale=` means it: how many physical
    /// pixels one logical pixel is drawn with. The output keeps its pixel size and apps get a
    /// smaller logical area, so text and controls come out proportionally bigger — which is
    /// what makes a desktop app usable on a phone-sized output. 1.0 leaves it alone.
    ///
    /// Not an encoder knob: the encoded frame is `width x height` whatever this says.
    #[serde(default = "default_scale")]
    pub scale: f32,
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
    /// Encoder backend preference (hardware vs software). Applied at session start.
    #[serde(default)]
    pub encoder: EncoderPref,
    /// Run the session's applications away from the host desktop. Applied at session start.
    ///
    /// On (the default), each session gets its own D-Bus session bus and its applications are
    /// launched with `DISPLAY` removed. Both halves answer the same complaint — "I launched it
    /// in wado and it opened on my computer's desktop" — by different routes:
    ///
    /// - **The bus.** A single-instance application (a browser, a file manager, most GTK apps)
    ///   checks the session bus for an existing copy of itself and, finding one, asks *it* to
    ///   open a window. That window belongs to the host's compositor. A private bus means
    ///   nothing is found and a real process starts inside the session.
    /// - **`DISPLAY`.** wado has no Xwayland, so an X11 client cannot draw here at all. With
    ///   `DISPLAY` inherited it connects to the host's X server instead and appears there —
    ///   and Chromium/Electron *prefer* X11 whenever `DISPLAY` is set, even with
    ///   `WAYLAND_DISPLAY` present. Removing it forces the Wayland backend, and an X11-only
    ///   application fails visibly rather than opening somewhere else.
    ///
    /// Defaults to true on both the wire and the UI: an application escaping to the host
    /// desktop is the bug, not the baseline.
    #[serde(default = "default_isolate_apps")]
    pub isolate_apps: bool,
    /// Give the session its own X server, so X11-only applications run inside it.
    ///
    /// Off by default, and the reason is visible rather than theoretical: the X server is
    /// rootful, so it puts a window the size of the output into the session whether or not
    /// anything is using it. Worth it when you are launching Steam, which cannot speak
    /// Wayland at all; not worth it otherwise.
    ///
    /// See `wado_compositor::session_env::xwayland` for what it can and cannot do — notably
    /// that every X application shares one screen with no window manager in it.
    #[serde(default)]
    pub x_server: bool,
}

/// Isolated. See [`SessionConfig::isolate_apps`].
fn default_isolate_apps() -> bool {
    true
}

/// Unscaled. Anything else is an explicit choice.
fn default_scale() -> f32 {
    1.0
}

impl SessionConfig {
    /// Reject a configuration that cannot work, before anything is built from it.
    ///
    /// **This is a trust boundary.** A `SessionConfig` arrives over a WebSocket from whoever
    /// knows the Remote ID; nothing between there and `Output::new` / the encoder's `open` had
    /// looked at it. A zero width is a divide-by-zero in the logical geometry, an odd width is
    /// invalid for 4:2:0 chroma and fails inside the encoder with a message about planes, and a
    /// 16-bit frame rate becomes a nanosecond timer interval of zero — a render loop that never
    /// yields to the input or Wayland sources.
    ///
    /// Lives here, on the type, rather than in either transport: the relay path and the HTTP
    /// path take the same struct from the same kind of source, and a guard added to one of them
    /// is a guard the other silently does not have.
    pub fn validate(&self) -> Result<(), String> {
        // H.264 4:2:0 subsamples chroma by two in both directions, so an odd dimension has no
        // valid chroma plane. Encoders report this as an internal error several layers down.
        if self.width < 160 || self.width > 7680 || self.width % 2 != 0 {
            return Err(format!("width {} is out of range (160-7680, even)", self.width));
        }
        if self.height < 120 || self.height > 4320 || self.height % 2 != 0 {
            return Err(format!("height {} is out of range (120-4320, even)", self.height));
        }
        if self.fps < 1 || self.fps > 240 {
            return Err(format!("fps {} is out of range (1-240)", self.fps));
        }
        if !self.scale.is_finite() || self.scale < 0.5 || self.scale > 4.0 {
            return Err(format!("scale {} is out of range (0.5-4.0)", self.scale));
        }
        if let Quality::Custom { bitrate_kbps } = self.quality {
            if !(100..=200_000).contains(&bitrate_kbps) {
                return Err(format!("bitrate {bitrate_kbps} kbps is out of range (100-200000)"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod config_validation_tests {
    use super::{Quality, SessionConfig};

    fn ok() -> SessionConfig {
        SessionConfig {
            isolate_apps: true,
            x_server: false,
            width: 1280, height: 720, fps: 60, scale: 1.0,
            quality: Quality::Balanced,
            preset: None, keyframe_interval: None,
            input: Default::default(), window: Default::default(), encoder: Default::default(),
        }
    }

    #[test]
    fn a_sane_config_passes() {
        assert!(ok().validate().is_ok());
    }

    #[test]
    fn odd_dimensions_are_rejected() {
        // The one that does not look like a bug until an encoder fails deep inside: 4:2:0 has
        // no valid chroma plane for an odd width.
        let mut c = ok();
        c.width = 1281;
        assert!(c.validate().is_err());
        let mut c = ok();
        c.height = 721;
        assert!(c.validate().is_err());
    }

    #[test]
    fn zero_and_absurd_sizes_are_rejected() {
        for (w, h) in [(0, 720), (1280, 0), (99999, 720), (1280, 99999)] {
            let mut c = ok();
            c.width = w;
            c.height = h;
            assert!(c.validate().is_err(), "{w}x{h} should not be allowed");
        }
    }

    #[test]
    fn a_zero_frame_rate_is_rejected() {
        // 1_000_000_000 / 0 in the render timer, and before that a divide in the sink.
        let mut c = ok();
        c.fps = 0;
        assert!(c.validate().is_err());
    }

    #[test]
    fn a_nonsense_scale_is_rejected() {
        for s in [0.0, -1.0, 9.0, f32::NAN, f32::INFINITY] {
            let mut c = ok();
            c.scale = s;
            assert!(c.validate().is_err(), "scale {s} should not be allowed");
        }
    }

    #[test]
    fn an_absurd_custom_bitrate_is_rejected() {
        let mut c = ok();
        c.quality = Quality::Custom { bitrate_kbps: 0 };
        assert!(c.validate().is_err());
        c.quality = Quality::Custom { bitrate_kbps: 5_000_000 };
        assert!(c.validate().is_err());
    }
}

/// Encoder-backend selection for a session (the "Compositor settings → encoder" group).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EncoderPref {
    /// Which encode backend to use. Defaults to [`EncoderBackend::Auto`] (probe and pick).
    #[serde(default)]
    pub backend: EncoderBackend,
}

/// Which video-encode backend a session should use.
///
/// `Auto` probes for a working hardware encoder and silently falls back to software
/// (with a user-facing banner — invariant #5). `Hardware` requires one (errors if none
/// opens). `Software` forces the CPU path (useful to exercise the fallback/banner).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncoderBackend {
    /// Probe for a hardware encoder; fall back to software if none works.
    #[default]
    Auto,
    /// Require a hardware encoder; fail session start if none opens.
    Hardware,
    /// Force the software (x264) encoder.
    Software,
}

/// What the server actually started, returned as the JSON body of a successful
/// `POST /session/start`. Lets the client surface the active encoder (e.g. a persistent
/// "software encoding" banner — invariant #5) without scraping the log stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// The encoder the server selected for this session.
    pub encoder: EncoderReport,
}

/// Describes the encoder a running session actually opened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderReport {
    /// Hardware or software — drives the client banner.
    pub mode: EncoderMode,
    /// Codec name, e.g. `"h264"`.
    pub codec: String,
    /// Concrete backend identifier, e.g. `"vaapi"` or `"x264"`.
    pub backend: String,
    /// Active pipeline tier id — `"vaapi-dmabuf"` (zero-copy), `"vaapi-cpu"`, or `"x264-cpu"`.
    /// Lets the client mark a *fallback* path. Defaults empty for older servers.
    #[serde(default)]
    pub pipeline: String,
    /// The CBR target the encoder was actually built with, in kbps.
    ///
    /// The client cannot derive this: `Quality::Balanced` is a word, and the kbps it becomes
    /// depends on the resolution and frame rate the *server* resolved. Without it the client
    /// can say what is arriving but not whether that is what was asked for — which is exactly
    /// the comparison that separates "the link is too small" from "the server stopped sending".
    #[serde(default)]
    pub bitrate_kbps: u32,
    /// The frame rate the encoder was built for. The decode budget is `1000 / fps`, and a
    /// decode time measured against the wrong budget accuses the wrong machine.
    #[serde(default)]
    pub fps: u32,
}

/// Whether a running session is encoding in hardware or software.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncoderMode {
    Hardware,
    Software,
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
        Self {
            repeat_rate: 25,
            repeat_delay: 200,
            focus_follows_pointer: false,
        }
    }
}

/// Window-management settings for a session (the "Compositor settings → window" group).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Where newly-mapped toplevels are placed on the output.
    #[serde(default)]
    pub placement: Placement,
}

/// What produced a scroll event.
///
/// Wheel is the default so an older client, which sent neither field, keeps behaving exactly
/// as it did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollSource {
    #[default]
    Wheel,
    Finger,
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
