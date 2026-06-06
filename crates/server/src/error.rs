//! The server crate's error type, covering the control plane (HTTP signaling,
//! WebRTC) and wrapping the compositor's [`wado_compositor::CompositorError`].
//!
//! `WadoError` is `Send + Sync` so it crosses the tokio/calloop boundary freely.
//! `main` returns `Box<dyn Error + Send + Sync>`; `WadoError` converts into it.

use thiserror::Error;

/// Convenience alias used across the server modules.
pub type Result<T> = std::result::Result<T, WadoError>;

#[derive(Debug, Error)]
pub enum WadoError {
    /// Any error from webrtc-rs (peer connection, track, SDP, write_sample, RTCP).
    #[error("webrtc error: {0}")]
    Webrtc(#[from] webrtc::Error),

    /// Socket / file IO.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// (De)serializing SDP or session config JSON.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// An error surfaced by the compositor library.
    #[error("compositor error: {0}")]
    Compositor(#[from] wado_compositor::CompositorError),

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
