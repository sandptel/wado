//! `wado` (server) — the always-on control plane: HTTP/WebRTC signaling, the WebRTC
//! video frame pump, and the live-log SSE stream. The compositor itself lives in the
//! `wado-compositor` crate; this crate drives it only through the typed command/frame
//! boundary (see [`website::start`]).

pub mod apps;
pub mod error;
pub mod exec;
pub mod relay_client;
pub mod pty;
pub mod remote_id;
pub mod webrtc_settings;
pub mod website;

pub use error::{Result, WadoError};
