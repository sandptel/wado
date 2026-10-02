//! `stream { }` — the host's say over encoding. Clients choose freely *inside* these limits.

use serde::{Deserialize, Serialize};
use wado_protocol::EncoderBackend;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Stream {
    /// `"auto"` lets the client choose. `"hardware"` / `"software"` pin it for every client —
    /// e.g. forcing x264 on a box whose VA-API driver is flaky.
    pub encoder: Encoder,
    pub max_fps: Option<u32>,
    /// kbps.
    pub max_bitrate: Option<u32>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Encoder {
    /// The client's choice stands.
    #[default]
    Auto,
    Hardware,
    Software,
}

impl Encoder {
    /// The backend to force, if any.
    pub fn forced(self) -> Option<EncoderBackend> {
        match self {
            Encoder::Auto => None,
            Encoder::Hardware => Some(EncoderBackend::Hardware),
            Encoder::Software => Some(EncoderBackend::Software),
        }
    }
}
