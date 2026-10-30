//! `wado` (server) — the always-on control plane: HTTP/WebRTC signaling, the WebRTC
//! video frame pump, and the live-log SSE stream. The compositor itself lives in the
//! `wado-compositor` crate; this crate drives it only through the typed command/frame
//! boundary (see [`website::start`]).

pub mod a11y;
pub mod actions;
pub mod apps;
pub mod audio;
pub mod claude;
pub mod cli;
pub mod clip;
pub mod config;
pub mod control;
pub mod e2e;
pub mod error;
pub mod files;
pub mod gate;
pub mod host;
pub mod ice;
pub mod input_order;
pub mod instance;
pub mod links;
pub mod menu_sheet;
pub mod nat;
pub mod notify;
pub mod pad;
pub mod panic_log;
pub mod passkey;
pub mod platform;
pub mod pty;
pub mod pumpstats;
pub mod push;
pub mod relay_client;
pub mod remote_id;
pub mod runlane;
pub mod sched;
pub mod sessions;
pub mod shells;
pub mod slot;
pub mod tui;
pub mod webrtc_settings;
pub mod website;

pub use error::{Result, WadoError};
