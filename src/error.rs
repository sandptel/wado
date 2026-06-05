//! The central error type for wado's streaming pipeline and control plane.
//!
//! One `WadoError` flows through the hot path — capture → encode → sink → WebRTC —
//! and the control plane (session lifecycle, HTTP signaling). It is `Send + Sync`
//! so it crosses the tokio/calloop boundary freely. Smithay's EGL/GLES error types
//! are many and concrete; rather than enumerate them, the call sites stringify them
//! into [`WadoError::Renderer`], keeping this enum small and stable.
//!
//! `main` still returns `Box<dyn Error + Send + Sync>`; `WadoError` converts into it.

use thiserror::Error;

/// Convenience alias used across the pipeline modules.
pub type Result<T> = std::result::Result<T, WadoError>;

#[derive(Debug, Error)]
pub enum WadoError {
    /// EGL/GLES context, renderbuffer, bind, or read-back failure.
    #[error("EGL/GLES setup failed: {0}")]
    Renderer(String),

    /// The x264 encoder could not be built or reconfigured.
    #[error("encoder build failed: {0}")]
    Encoder(String),

    /// Reading pixels back from the framebuffer failed.
    #[error("frame capture failed: {0}")]
    Capture(String),

    /// Any error from webrtc-rs (peer connection, track, SDP, write_sample, RTCP).
    #[error("webrtc error: {0}")]
    Webrtc(#[from] webrtc::Error),

    /// Socket / file IO.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// (De)serializing SDP or session config JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The session config from the web client was malformed or impossible.
    #[error("invalid session config: {0}")]
    Config(String),

    /// `/session/start` while a session is already running.
    #[error("a session is already active")]
    SessionAlreadyActive,

    /// An operation needed an active session but none exists.
    #[error("no active session")]
    NoActiveSession,

    /// Anything that does not fit the categories above.
    #[error("{0}")]
    Other(String),
}

impl From<String> for WadoError {
    fn from(s: String) -> Self {
        WadoError::Other(s)
    }
}

impl From<&str> for WadoError {
    fn from(s: &str) -> Self {
        WadoError::Other(s.to_string())
    }
}
