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
/// `POST /session/start`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub quality: Quality,
    /// Optional initial command launched when the session starts (program + args,
    /// space-split). Empty means start a blank session; more commands can be spawned
    /// at runtime via [`endpoints::SESSION_LAUNCH`].
    pub command: String,
    /// Advanced override: x264 preset name ("ultrafast".."veryfast"). Falls back to
    /// the quality preset's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Advanced override: frames between IDR keyframes. Falls back to the quality
    /// preset's default when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyframe_interval: Option<u32>,
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
