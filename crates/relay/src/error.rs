//! Relay error type.

use thiserror::Error;

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum RelayError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("server not found: {0}")]
    ServerNotFound(String),
    #[error("server already registered: {0}")]
    AlreadyRegistered(String),
    #[error("room full: server {0} already has an active client")]
    RoomFull(String),
    #[error("auth failed")]
    AuthFailed,
    #[error("{0}")]
    Other(String),
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, RelayError>;
