//! `wado-platform` — every host capability wado uses, as a trait, behind a registry that says
//! honestly what is missing (`docs/superpowers/specs/2026-10-30-platform-layer-design.md`).
//!
//! It exists so a macOS or Windows backend, or a desktop app, can provide or consume host
//! capabilities selectively, and so every capability can be faked in tests. Providers live
//! elsewhere (the Linux ones in the server crate, `platform/linux/`); this crate holds only the
//! seams. It names no Smithay type and no network type.
//!
//! Two shapes of interface, by how hot the path is:
//!
//! - **Devices** ([`input`]): small synchronous traits that never await — a touch frame or a
//!   key tap goes straight to the device. Opened through an [`input::Opener`], so each device is
//!   owned by one input lane. Key and button codes are Linux evdev codes everywhere; another
//!   platform maps them to its own.
//! - **Control plane** (the rest): one trait per capability family, async methods returning a
//!   hand-written boxed future ([`Fut`]) with owned arguments. A stateful capability publishes
//!   its latest state on a `watch` channel ([`state::Published`]), so readers never cause I/O.
//!
//! Files, one job each:
//!
//! | | |
//! |---|---|
//! | [`cap`] | `Cap`, `Absent`, `ProviderId` — a capability provided, or why not |
//! | [`registry`] | `Provider`, `Platform`, `PlatformBuilder` — probe once, cache, re-probe |
//! | [`state`] | `Published<S>` — latest state on a watch, refreshed at most as often as asked |
//! | [`service`] | a backend on its own thread behind a `Port`, restarted if it panics |
//! | [`input`] | touch, keys, gamepad devices; typing text |
//! | [`audio`] | the mixer, and the sound output that plays to the viewer |
//! | [`media`] | media players |
//! | [`display`] | the screen's backlight |
//! | [`network`] | Wi-Fi, Bluetooth |
//! | [`power`] | keeping awake; lock and sleep |
//! | [`clip`] | the clipboard and its history |
//! | [`desktop`] | windows and workspaces, opening apps, screenshots |
//! | [`sysinfo`] | what the computer is |
//! | [`files`] | the trash, thumbnails |
//! | [`pty`] | shells on a pseudo-terminal |
//! | [`fake`] | a fake for every capability, and the contract providers must pass (`fake` feature) |

pub mod audio;
pub mod cap;
pub mod clip;
pub mod desktop;
pub mod display;
pub mod files;
pub mod input;
pub mod media;
pub mod network;
pub mod power;
pub mod pty;
pub mod registry;
pub mod service;
pub mod state;
pub mod sysinfo;

#[cfg(any(test, feature = "fake"))]
pub mod fake;

use std::future::Future;
use std::pin::Pin;

pub use cap::{Absent, Cap, ProviderId};
pub use registry::{Platform, PlatformBuilder, Provider};
pub use state::Published;
pub use wado_protocol::caps::{CapSet, Capability};

/// What a control-plane method returns: a boxed future, so the traits stay object-safe without
/// an `async-trait` dependency. Arguments are owned, so the future borrows only `self`.
pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The outcome of a control-plane operation: on failure, the reason in words a person can act on.
pub type Res<T> = Result<T, String>;
