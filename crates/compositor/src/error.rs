//! The compositor crate's error type.
//!
//! `CompositorError` flows through the render hot path — capture → encode → sink —
//! and the session lifecycle. It is `Send + Sync` so it crosses the calloop boundary
//! freely. Smithay's EGL/GLES error types are many and concrete; rather than enumerate
//! them, the call sites stringify them into [`CompositorError::Renderer`], keeping this
//! enum small and stable.
//!
//! The server crate wraps this in its own `WadoError` (which additionally carries
//! WebRTC/JSON variants) via a `#[from]` conversion.

use thiserror::Error;

/// Convenience alias used across the compositor modules.
pub type Result<T> = std::result::Result<T, CompositorError>;

#[derive(Debug, Error)]
pub enum CompositorError {
    /// EGL/GLES context, renderbuffer, bind, or read-back failure.
    #[error("EGL/GLES setup failed: {0}")]
    Renderer(String),

    /// The x264 encoder could not be built or reconfigured.
    #[error("encoder build failed: {0}")]
    Encoder(String),

    /// Reading pixels back from the framebuffer failed.
    #[error("frame capture failed: {0}")]
    Capture(String),

    /// Socket / file IO.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The session config was malformed or impossible.
    #[error("invalid session config: {0}")]
    Config(String),

    /// `start_session` while a session is already running.
    #[error("a session is already active")]
    SessionAlreadyActive,

    /// An operation needed an active session but none exists.
    #[error("no active session")]
    NoActiveSession,

    /// Anything that does not fit the categories above.
    #[error("{0}")]
    Other(String),
}

impl From<String> for CompositorError {
    fn from(s: String) -> Self {
        CompositorError::Other(s)
    }
}

impl From<&str> for CompositorError {
    fn from(s: &str) -> Self {
        CompositorError::Other(s.to_string())
    }
}
