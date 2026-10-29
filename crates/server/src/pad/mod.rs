//! Trackpad mode — a phone acts as this computer's multi-touch touchpad and keyboard, with no
//! picture and no session (`docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`). Touch
//! and key messages ride their own WebRTC data channels (`wado_protocol::pad`) straight into a
//! kernel input device, so every app on the host sees an ordinary trackpad.
//!
//! Files, one job each:
//! - `uinput.rs` — the raw `/dev/uinput` device: open, describe, emit events, destroy on drop.
//! - `touchpad.rs` — the virtual touchpad: device setup, and the pure slot-assignment logic
//!   that turns a `Contact` frame into evdev events.
//! - `keys.rs` — the virtual keyboard: US-layout key/char lookup, chord construction, and the
//!   device that tracks what it holds.
//! - `text.rs` — committed text, typed into the desktop via `wtype`.
//! - `state.rs` — one phone's pad: what each message does, stale frames, silence, resizes.
//! - `pacer.rs` — touch frames written on a steady cadence, interpolated, by either path.
//! - `delay.rs` — how far behind the phone the pacer plays: measured, or fixed.
//! - `arrivals.rs` — how evenly frames arrive, logged so jitter is measured, not assumed.
//! - `sink.rs` — the kernel devices a pad drives, made on first use.
//! - `gate.rs` — who may use the trackpad, asked of every message.
//! - `live.rs` — the open pad, fed by either transport, its tick and its ordered keyboard queue.
//! - `channel.rs` — the direct link: a peer connection with a touch and a keys channel.
//!
//! Through the relay the same messages arrive as `RelayMsg::PadIn` (`relay_client.rs`), when
//! the phone has no direct path or before its direct link is up.

pub mod arrivals;
pub mod channel;
pub mod delay;
pub mod gate;
pub mod keys;
pub mod live;
pub mod pacer;
pub mod sink;
pub mod state;
pub mod text;
pub mod touchpad;
pub mod uinput;

pub use live::{Live, Peer};
