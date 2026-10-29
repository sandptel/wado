//! Trackpad mode — a phone acts as this computer's multi-touch touchpad and keyboard, with no
//! picture and no session (`docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`). Touch
//! and key messages ride their own WebRTC data channels (`wado_protocol::pad`) straight into a
//! kernel input device, so every app on the host sees an ordinary trackpad.
//!
//! Files, one job each:
//! - `uinput.rs` — the raw `/dev/uinput` device: open, describe, emit events, destroy on drop.
//! - `touchpad.rs` — the virtual touchpad: device setup, and the pure slot-assignment logic
//!   that turns a `Contact` frame into evdev events.

pub mod touchpad;
pub mod uinput;
