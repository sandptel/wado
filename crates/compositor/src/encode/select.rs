//! Pipeline tier selection. A *tier* couples a capture strategy with an encoder; tiers are
//! tried top-down (zero-copy HW → CPU-upload HW → software) and the first that opens wins.
//! The concrete build (which needs the renderer + GBM device) lives in
//! [`crate::headless`]; this module only decides **which tiers are eligible** for a given
//! [`EncoderBackend`] preference, and reports each tier.
//!
//! - `vaapi-dmabuf` (Tier A): DMA-BUF zero-copy → VAAPI VPP → `h264_vaapi`.
//! - `vaapi-cpu`   (Tier B): `ExportMem` → CPU NV12 → upload → `h264_vaapi`.
//! - `x264-cpu`    (Tier C): `ExportMem` → x264 software.

use wado_protocol::{EncoderBackend, EncoderMode, EncoderReport, KeyframeMode};

/// One pipeline tier, best (lowest latency) first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    VaapiDma,
    VaapiCpu,
    X264,
}

impl Tier {
    /// The wire report for this tier, given the engaged `keyframe_mode`.
    pub fn report(self, keyframe_mode: KeyframeMode) -> EncoderReport {
        let (mode, codec, backend, pipeline) = match self {
            Tier::VaapiDma => (EncoderMode::Hardware, "h264", "vaapi", "vaapi-dmabuf"),
            Tier::VaapiCpu => (EncoderMode::Hardware, "h264", "vaapi", "vaapi-cpu"),
            Tier::X264 => (EncoderMode::Software, "h264", "x264", "x264-cpu"),
        };
        let keyframe = match keyframe_mode {
            KeyframeMode::OnDemand => "on-demand",
            KeyframeMode::Periodic => "periodic",
        };
        EncoderReport {
            mode,
            codec: codec.into(),
            backend: backend.into(),
            pipeline: pipeline.into(),
            keyframe: keyframe.into(),
        }
    }

    /// The next tier to try after this one fails at runtime (downgrade-once).
    pub fn next(self) -> Option<Tier> {
        match self {
            Tier::VaapiDma => Some(Tier::VaapiCpu),
            Tier::VaapiCpu => Some(Tier::X264),
            Tier::X264 => None,
        }
    }
}

/// The ordered list of tiers to attempt for a backend preference. `Hardware` omits the
/// software tier (it must use the GPU or fail); `Auto` includes everything; `Software`
/// forces x264. The DMA tier is only listed when a GBM device is available.
pub fn tiers_for(backend: EncoderBackend, gbm_available: bool) -> Vec<Tier> {
    let mut tiers = Vec::new();
    match backend {
        EncoderBackend::Software => tiers.push(Tier::X264),
        EncoderBackend::Hardware => {
            if gbm_available {
                tiers.push(Tier::VaapiDma);
            }
            tiers.push(Tier::VaapiCpu);
        }
        EncoderBackend::Auto => {
            if gbm_available {
                tiers.push(Tier::VaapiDma);
            }
            tiers.push(Tier::VaapiCpu);
            tiers.push(Tier::X264);
        }
    }
    tiers
}
