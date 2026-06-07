//! Video encoders. [`encoder::VideoEncoder`] is the backend-agnostic trait; [`select`]
//! picks a concrete backend per the session's [`wado_protocol::EncoderBackend`]
//! preference (probing hardware, falling back to software). Backends:
//! [`x264enc`] (software) and [`ffmpeg`] (VAAPI hardware).

pub mod encoder;
pub mod ffmpeg;
pub mod select;
pub mod x264enc;
