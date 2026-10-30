//! The computer's clipboard, shared with the devices — the desktop's, a wado session's, and every
//! paired phone's that may (the `clipboard` grant), kept in step without echo, and never a secret.
//!
//! The history is the desktop's own `cliphist` (fed by the desktop's `wl-paste --watch cliphist
//! store`) when it has one, so whatever the person copied on the computer is there, and anything
//! written here with `wl-copy` lands in it too; without it, wado keeps the last 50 in memory
//! ([`ram`]). The desktop is reached with its `WAYLAND_DISPLAY`, captured before the compositor
//! replaces it ([`env`]).
//!
//! The clipboard and its history are the platform's (`wado_platform::clip`, provided on Linux by
//! `crate::platform::linux::{wl_clipboard, cliphist}`); this is the policy over them.
//!
//! - [`env`] — the desktop's Wayland display.
//! - [`watch`] — notice host clipboard changes.
//! - [`hub`] — every copy noted once: new, an echo, or a secret.
//! - [`sync`] — host ⇄ session ⇄ a device's push, through the hub.
//! - [`ram`] — wado's own history, in memory.
//! - [`share`] — following the clipboard for one link: new copies and the history.
//! - [`data`] — bytes ⇄ the wire's text or `data:` URI.
//! - [`serve`] — answering a device's clipboard messages.

pub mod data;
pub mod env;
pub mod hub;
pub mod ram;
pub mod serve;
pub mod share;
pub mod sync;
pub mod watch;
