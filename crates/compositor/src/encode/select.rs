//! Backend selection: turn the session's [`EncoderBackend`] preference into a concrete
//! [`VideoEncoder`] plus the [`EncoderReport`] the server returns to the client.
//!
//! - `Software` → x264 (CPU).
//! - `Hardware` → VAAPI; **errors** if no working hardware encoder opens.
//! - `Auto` → VAAPI if the cached probe says one works, else silently fall back to x264
//!   with a `warn!` (the client also shows a banner — invariant #5).

use wado_protocol::{EncoderBackend, EncoderMode, EncoderReport};

use crate::CompositorError;
use crate::conf::EncoderConfig;
use crate::encode::encoder::VideoEncoder;
use crate::encode::ffmpeg::{FfmpegVaapiEncoder, hwcontext::first_render_node, probe};
use crate::encode::x264enc::X264Encoder;

/// Build the encoder for a session and report what was actually opened.
pub fn build_encoder(ec: &EncoderConfig) -> crate::Result<(Box<dyn VideoEncoder>, EncoderReport)> {
    match ec.backend {
        EncoderBackend::Software => Ok((build_x264(ec)?, software_report())),
        EncoderBackend::Hardware => Ok((build_vaapi(ec)?, hardware_report())),
        EncoderBackend::Auto => {
            if probe::vaapi_available() {
                match build_vaapi(ec) {
                    Ok(enc) => return Ok((enc, hardware_report())),
                    Err(e) => tracing::warn!(
                        "hardware encoder build failed after a successful probe — \
                         falling back to software: {e}"
                    ),
                }
            } else {
                tracing::warn!("no working hardware encoder — using software (x264) fallback");
            }
            Ok((build_x264(ec)?, software_report()))
        }
    }
}

fn build_x264(ec: &EncoderConfig) -> crate::Result<Box<dyn VideoEncoder>> {
    let enc = X264Encoder::new(
        ec.width,
        ec.height,
        ec.fps,
        ec.bitrate_kbps,
        ec.keyframe_interval,
        ec.preset,
    )?;
    Ok(Box::new(enc))
}

fn build_vaapi(ec: &EncoderConfig) -> crate::Result<Box<dyn VideoEncoder>> {
    let node = first_render_node()
        .ok_or_else(|| CompositorError::Encoder("no DRM render node (/dev/dri/renderD*)".into()))?;
    let enc = FfmpegVaapiEncoder::new(
        &node,
        ec.width,
        ec.height,
        ec.fps,
        ec.bitrate_kbps,
        ec.keyframe_interval,
    )?;
    Ok(Box::new(enc))
}

fn hardware_report() -> EncoderReport {
    EncoderReport { mode: EncoderMode::Hardware, codec: "h264".into(), backend: "vaapi".into() }
}

fn software_report() -> EncoderReport {
    EncoderReport { mode: EncoderMode::Software, codec: "h264".into(), backend: "x264".into() }
}
