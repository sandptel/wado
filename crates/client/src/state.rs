//! Every piece of UI state, and nothing else — no rendering, no side effects.
//!
//! Split in two on one axis: [`Settings`] is what the user chose and what therefore survives
//! a reload; [`Live`] is what the session is currently doing and is meaningless once it ends.
//! Keeping them apart is what lets [`crate::persist`] serialise "the settings" without having
//! to remember, field by field, that a frame counter is not a setting.
//!
//! Both are `Copy`: a `Signal` is a handle, not the value, so passing these around is free and
//! every holder sees the same state.

use dioxus::prelude::*;
use wado_protocol::{logfmt::LogLine, AppEntry};

/// Default server the client talks to. Editable in the UI; the dev server typically runs the
/// client on a different port and reaches the wado server here over CORS.
pub const DEFAULT_SERVER: &str = "http://127.0.0.1:8080";

/// Default relay the client dials in relay mode. Editable in the UI.
/// ponytail: a trycloudflare quick tunnel — ephemeral, it changes every `cloudflared`
/// restart. Replace when the relay gets a stable hostname.
pub const DEFAULT_RELAY: &str = "https://accompanying-dec-wind-release.trycloudflare.com";

/// Marks a scale that has never been chosen — neither by the user nor from pixel density.
/// Not a valid option value, so it cannot survive the effect that resolves it.
pub const SCALE_UNSET: &str = "";

/// Keep at most this many log lines in memory / the DOM.
pub const MAX_LOG_LINES: usize = 500;

/// User choices. Everything here is persisted by [`crate::persist`].
///
/// Grouped in the UI by *when it takes effect* — connection, session (needs a restart), live
/// (instant), appearance, debug — rather than by subsystem, because "will this apply now or
/// at Start?" is the question people actually get wrong.
#[derive(Clone, Copy)]
pub struct Settings {
    // ── connection ──────────────────────────────────────────────────────────────
    /// "direct" (HTTP straight to wado-server) or "relay" (everything via wado-relay).
    pub conn_mode: Signal<String>,
    pub server_addr: Signal<String>,
    pub relay_url: Signal<String>,
    pub remote_id: Signal<String>,

    // ── session: read once at Start, locked while a session runs ────────────────
    pub res: Signal<String>,
    pub custom_w: Signal<u32>,
    pub custom_h: Signal<u32>,
    pub scale: Signal<String>,
    pub fps: Signal<u32>,
    pub quality: Signal<String>,
    pub bitrate: Signal<u32>,
    pub encoder_backend: Signal<String>,
    pub placement: Signal<String>,
    pub focus_follows: Signal<bool>,
    pub repeat_rate: Signal<i32>,
    pub repeat_delay: Signal<i32>,
    pub preset: Signal<String>,
    pub keyframe: Signal<String>,

    // ── live: applied immediately, editable mid-session ─────────────────────────
    pub command: Signal<String>,
    pub move_mode: Signal<bool>,
    pub scroll_speed: Signal<f64>,
    pub natural_scroll: Signal<bool>,

    // ── appearance ──────────────────────────────────────────────────────────────
    /// Bundled base16 scheme name; ignored while `theme_custom` parses.
    /// Whether the docked desktop panel is showing. Only meaningful above the layout
    /// breakpoint — below it the panel is a sheet and `Live::sheet_open` governs instead.
    ///
    /// Lives here rather than in `Live` because it is persisted, and persistence reads this
    /// struct. Persisted unlike `sheet_open`, for a reason: a sheet covering the video on
    /// load is never what anyone wanted, but someone who collapsed the panel means it.
    pub panel_open: Signal<bool>,

    pub theme: Signal<String>,
    /// Raw text of a pasted base16 scheme. Kept verbatim so the box still shows what was
    /// pasted after a reload, even though only the parsed values are applied.
    pub theme_custom: Signal<String>,

    // ── debug ───────────────────────────────────────────────────────────────────
    /// Master switch for the whole debug group.
    pub debug_master: Signal<bool>,
    /// One flag per [`crate::debug::ITEMS`] entry, index-aligned. Persisted by `id`, so
    /// reordering or removing an item cannot scramble the rest.
    pub debug: Signal<Vec<bool>>,
}

impl Settings {
    /// Must be called from inside a component — these are hooks.
    pub fn new() -> Self {
        Self {
            conn_mode: use_signal(|| "relay".to_string()),
            server_addr: use_signal(|| DEFAULT_SERVER.to_string()),
            relay_url: use_signal(|| DEFAULT_RELAY.to_string()),
            remote_id: use_signal(String::new),

            // Empty on purpose: no fixed resolution is the right default when the right one
            // depends on the screen. The effect in `main` fills it with the device-exact
            // option as soon as the bridge reports the screen, and an empty value is not on
            // offer so it can never survive that.
            res: use_signal(String::new),
            custom_w: use_signal(|| 1280),
            custom_h: use_signal(|| 720),
            // Sentinel, not a value: the right scale depends on the device's pixel density,
            // which the bridge has not reported yet. Replaced in `main`'s effect.
            scale: use_signal(|| SCALE_UNSET.to_string()),
            fps: use_signal(|| 60),
            quality: use_signal(|| "balanced".to_string()),
            bitrate: use_signal(|| 4000),
            encoder_backend: use_signal(|| "auto".to_string()),
            placement: use_signal(|| "center".to_string()),
            focus_follows: use_signal(|| false),
            repeat_rate: use_signal(|| 25),
            repeat_delay: use_signal(|| 200),
            preset: use_signal(String::new),
            keyframe: use_signal(String::new),

            command: use_signal(|| "weston-terminal".to_string()),
            move_mode: use_signal(|| false),
            // See ui/live.rs: 1.0 meant "pass the raw browser delta through", which is
            // too fast everywhere. Acceleration covers the range this gives up.
            scroll_speed: use_signal(|| 0.35),
            natural_scroll: use_signal(|| false),

            panel_open: use_signal(|| true),
            theme: use_signal(|| "default-dark".to_string()),
            theme_custom: use_signal(String::new),

            debug_master: use_signal(|| true),
            debug: use_signal(|| crate::debug::ITEMS.iter().map(|i| i.default).collect()),
        }
    }
}

/// What the running session is doing. Reset on stop; never persisted.
#[derive(Clone, Copy)]
pub struct Live {
    /// True once the saved settings blob has been applied.
    ///
    /// Exists because effects run before the bridge's async load completes: without this
    /// gate the persist effect fires on mount with the defaults still in place and writes
    /// them over the saved blob, so nothing would ever survive a reload.
    pub loaded: Signal<bool>,
    pub session_on: Signal<bool>,
    pub status: Signal<String>,
    pub stagebar: Signal<String>,
    pub logs: Signal<Vec<LogLine>>,
    /// Whether the console sheet is up, and which half of it is showing.
    ///
    /// One sheet with two tabs rather than two panels, and floating rather than stacked: as
    /// siblings under the video they each took height off the picture, so turning either on
    /// letterboxed the stream. On a phone there is no height to spare for a panel you are not
    /// reading.
    pub console_open: Signal<bool>,
    pub console_tab: Signal<String>,

    /// Terminal output: `(text, is_stderr)`. Capped like the log, for the same reason — an
    /// unbounded command would otherwise grow the DOM until the page dies.
    ///
    /// Kept apart from the log rather than interleaved: they answer different questions, and
    /// a command's output mixed into the server's tracing makes both harder to read.
    pub term: Signal<Vec<(String, bool)>>,
    /// What is typed in the console, kept apart from `Settings::command`.
    ///
    /// They were the same signal, so typing in the console rewrote the launcher's field in
    /// the settings panel and the other way round. They look alike and are not: the launcher
    /// holds a saved application to start with a session, the console holds a line you are
    /// typing right now. Not persisted, for the same reason.
    pub term_input: Signal<String>,
    /// True while a command is running, so the input can say so and refuse a second one.
    pub term_busy: Signal<bool>,
    /// Whether the settings sheet is up. Only meaningful below the layout breakpoint — above
    /// it the panel is docked and this is ignored. Not persisted: reopening a page with the
    /// settings sheet already covering the video is never what someone wanted.
    pub sheet_open: Signal<bool>,

    /// What the server actually opened, from the `/session/start` reply: the hw/sw `mode`
    /// drives the persistent software banner (invariant #5), the `pipeline` tier id drives
    /// the stagebar badge.
    pub encoder_mode: Signal<String>,
    pub encoder_pipeline: Signal<String>,

    /// The viewing device's physical screen in real pixels, once the bridge reports it.
    /// Drives the device-exact resolution options — see `crate::res`.
    /// Percentage of received frames this device failed to render, smoothed by the bridge.
    /// Distinct from `dropped`, which counts frames the *server* discarded: this one says
    /// the pipeline delivered and the viewer could not keep up.
    pub decode_drop_pct: Signal<f64>,

    pub screen_w: Signal<u32>,
    pub screen_h: Signal<u32>,
    /// The device's pixel density. Drives the default output scale the way a desktop
    /// compositor does: a phone reporting 2.6 wants roughly 3x, not 1x.
    pub screen_dpr: Signal<f64>,

    /// How far the connection got, as a count of completed stages (see `ui::status`).
    /// Relay mode reaches the video through four separate hops that fail for unrelated
    /// reasons, and a single status line cannot say which one you are stuck at.
    pub conn_stage: Signal<u8>,
    /// Why the connection stopped where it did. Empty while nothing has failed.
    pub conn_error: Signal<String>,

    /// Launchable applications, from the server. Empty until requested — and it stays empty
    /// on a server that could not be reached, which the free-text command box covers.
    pub apps: Signal<Vec<AppEntry>>,

    pub fps: Signal<Option<f64>>,
    pub ping: Signal<Option<f64>>,
    /// Receiver playout-buffer depth in ms — latency `ping` cannot see.
    pub jbuf: Signal<Option<f64>>,
    /// Per-stage breakdown as (label, ms) in pipeline order; empty until the bridge reports.
    pub stages: Signal<Vec<(String, f64)>>,
    pub dropped: Signal<Option<u64>>,
}

impl Live {
    pub fn new() -> Self {
        Self {
            loaded: use_signal(|| false),
            session_on: use_signal(|| false),
            status: use_signal(|| "idle".to_string()),
            stagebar: use_signal(|| "No session.".to_string()),
            logs: use_signal(Vec::new),
            console_open: use_signal(|| false),
            console_tab: use_signal(|| "shell".to_string()),
            term: use_signal(Vec::new),
            term_input: use_signal(String::new),
            term_busy: use_signal(|| false),
            sheet_open: use_signal(|| false),
            encoder_mode: use_signal(String::new),
            encoder_pipeline: use_signal(String::new),
            decode_drop_pct: use_signal(|| 0.0),
            screen_w: use_signal(|| 0),
            screen_dpr: use_signal(|| 0.0),
            screen_h: use_signal(|| 0),
            conn_stage: use_signal(|| 0),
            conn_error: use_signal(String::new),
            apps: use_signal(Vec::new),
            fps: use_signal(|| None),
            ping: use_signal(|| None),
            jbuf: use_signal(|| None),
            stages: use_signal(Vec::new),
            dropped: use_signal(|| None),
        }
    }

    /// Drop every per-session reading. Called on stop, on a failed start, and on give-up, so
    /// the stagebar never shows a number left over from a session that is gone.
    pub fn clear_telemetry(&mut self) {
        self.fps.set(None);
        self.ping.set(None);
        self.jbuf.set(None);
        self.stages.set(Vec::new());
        self.dropped.set(None);
        self.encoder_mode.set(String::new());
        self.encoder_pipeline.set(String::new());
    }
}

/// The two halves together — what every UI function receives.
#[derive(Clone, Copy)]
pub struct Ui {
    pub set: Settings,
    pub live: Live,
}
