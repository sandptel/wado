//! Remote input synthesis, split one job per file. `remote.rs` dispatches each
//! `wado_protocol::InputEvent` to a per-feature synthesizer; `common.rs` holds the shared
//! plumbing they all use. **No on-screen cursor** is ever drawn (a mouse drives a
//! cursorless `wl_pointer`; touchscreens drive `wl_touch`).

pub mod common;
pub mod keyboard;
pub mod pointer;
pub mod remote;
pub mod touch;
pub mod window_drag;
