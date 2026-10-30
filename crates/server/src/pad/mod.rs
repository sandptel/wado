//! Trackpad mode — a phone acts as this computer's multi-touch touchpad, keyboard and gamepad, with no
//! picture and no session (`docs/superpowers/specs/2026-10-29-trackpad-mode-design.md`). Touch
//! and key messages ride their own WebRTC data channels (`wado_protocol::pad`) straight into a
//! kernel input device, so every app on the host sees an ordinary trackpad.
//!
//! Files, one job each:
//! - `keys.rs` — what a key message means as a key: named keys and US-layout characters.
//! - `state.rs` — one phone's pad: what each message does, stale frames, silence, resizes.
//! - `pacer.rs` — touch frames written on a steady cadence, interpolated, by either path.
//! - `ticker.rs` — the pad's tick: on that cadence while anything is in flight, parked while idle.
//! - `delay.rs` — how far behind the phone the pacer plays: measured, or fixed.
//! - `arrivals.rs` — how evenly frames arrive, logged so jitter is measured, not assumed.
//! - `sink.rs` — the devices a pad drives, made on first use.
//! - `feel.rs` — when a phone's touchpad settings (speed, scroll, tapping) are applied on the
//!   computer: at its pad's first size, and when it changes them while the pad is open.
//! - `game.rs` — the sessionless gamepad: made on first use, let go when the screen closes,
//!   removed 30 s later; axes centred after silence.
//! - `gate.rs` — who may use the trackpad, asked of every message.
//! - `actions.rs` — listing and running the computer's actions (`crate::actions`), on `host`.
//! - `live.rs` — the open pad, fed by either transport, its tick and its ordered keyboard queue.
//! - `channel.rs` — the direct link: a peer connection with a touch and a keys channel.
//! - `bin.rs` — compact touch frames (`wado_protocol::pad_bin`) back into frames.
//!
//! The devices themselves — touchpad, keyboard, typist — are the platform's
//! (`wado_platform::input`, provided on Linux by `crate::platform::linux`).
//!
//! Through the relay the same messages arrive as `RelayMsg::PadIn` (`relay_client.rs`), when
//! the phone has no direct path or before its direct link is up.

pub mod actions;
pub mod arrivals;
pub mod bin;
pub mod channel;
pub mod delay;
pub mod feel;
pub mod game;
pub mod gate;
pub mod keys;
pub mod live;
pub mod pacer;
pub mod sink;
pub mod state;
pub mod ticker;

pub use live::{Live, Peer};
