//! The session's compositor, as the server names it: the one file that knows which OS this is.
//!
//! Linux: the Smithay compositor (`wado-compositor`). macOS: the boundary with no session behind
//! it yet (`wado-session-macos`), which answers every session command as coming to macOS. The
//! rest of the server says `crate::compositor::…` and builds on both.

#[cfg(target_os = "linux")]
pub use wado_compositor::*;

#[cfg(target_os = "macos")]
pub use wado_session_macos::*;

/// This OS's video backend, as `main` builds it.
#[cfg(target_os = "linux")]
pub use wado_compositor::backend::Smithay as Backend;
#[cfg(target_os = "macos")]
pub use wado_session_macos::backend::Session as Backend;
