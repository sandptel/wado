//! Hardware-encoder probe (invariant #6): never trust GPU/driver strings — actually
//! **open** a VAAPI H.264 encoder and run one frame through it, then **cache** the result
//! for the life of the process so we probe at most once.

use std::borrow::Cow;
use std::sync::OnceLock;

use super::FfmpegVaapiEncoder;
use super::hwcontext::first_render_node;
use crate::capture::Frame;
use crate::encode::encoder::VideoEncoder;

static VAAPI_OK: OnceLock<bool> = OnceLock::new();

/// Is a working VAAPI H.264 encoder available? Probes once (opens an encoder on the first
/// render node and pushes a single black frame) and caches the boolean. Cheap on repeat.
pub fn vaapi_available() -> bool {
    *VAAPI_OK.get_or_init(probe)
}

fn probe() -> bool {
    let Some(node) = first_render_node() else {
        tracing::warn!("VAAPI probe: no /dev/dri/renderD* node found");
        return false;
    };
    // A small encoder is enough to prove the device + codec + surface upload all work.
    // Use 320×240 (the size the proven `h264_vaapi` CLI test used): VAAPI encoders impose
    // a minimum frame size (e.g. AMD requires ≥128 per side), so don't probe too small.
    const PW: u32 = 320;
    const PH: u32 = 240;
    match FfmpegVaapiEncoder::new(&node, PW, PH, 30, 1000, 30, wado_protocol::KeyframeMode::OnDemand)
    {
        Ok(mut enc) => {
            let black = vec![0u8; (PW * PH * 4) as usize];
            match enc.submit(Frame::Rgba(Cow::Borrowed(&black))) {
                Ok(_) => {
                    tracing::info!(node, "VAAPI probe: hardware H.264 encoder opened OK");
                    true
                }
                Err(e) => {
                    tracing::warn!(node, "VAAPI probe: encoder opened but encode failed: {e}");
                    false
                }
            }
        }
        Err(e) => {
            tracing::warn!(node, "VAAPI probe: could not open hardware encoder: {e}");
            false
        }
    }
}
