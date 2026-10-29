//! The host computer's clipboard, for the landing page's clipboard rail — and kept in step with
//! the session's.
//!
//! The history is the desktop's own `cliphist` (fed by the desktop's `wl-paste --watch cliphist
//! store`), so whatever the person copied on the computer is there, and anything written here
//! with `wl-copy` lands in it too. Reached with the desktop's `WAYLAND_DISPLAY`, captured before
//! the compositor replaces it ([`env`]).
//!
//! The clipboard and its history are the platform's (`wado_platform::clip`, provided on Linux by
//! `crate::platform::linux::{wl_clipboard, cliphist}`); this is the policy over them.
//!
//! - [`env`] — the desktop's Wayland display.
//! - [`watch`] — notice host clipboard changes.
//! - [`sync`] — host ⇄ session, without echo.
//! - [`data`] — bytes ⇄ the wire's text or `data:` URI.
//! - [`serve`] — answering the viewer's clipboard messages.

pub mod data;
pub mod env;
pub mod serve;
pub mod sync;
pub mod watch;
