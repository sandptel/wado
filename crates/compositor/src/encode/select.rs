//! Pipeline tier selection. A *tier* couples a capture strategy with an encoder; tiers are
//! tried top-down (zero-copy HW → CPU-upload HW → software) and the first that opens wins.
//! The concrete build (which needs the renderer + GBM device) lives in
//! [`crate::headless`]; this module only decides **which tiers are eligible** for a given
//! [`EncoderBackend`] preference, and reports each tier.
//!
//! - `vaapi-dmabuf` (Tier A): DMA-BUF zero-copy → VAAPI VPP → `h264_vaapi`.
//! - `vaapi-cpu`   (Tier B): `ExportMem` → CPU NV12 → upload → `h264_vaapi`.
//! - `x264-cpu`    (Tier C): `ExportMem` → x264 software.

use wado_protocol::{EncoderBackend, EncoderMode, EncoderReport};

/// One pipeline tier, best (lowest latency) first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    VaapiDma,
    VaapiCpu,
    X264,
}

impl Tier {
    /// The wire report for this tier.
    pub fn report(self) -> EncoderReport {
        let (mode, codec, backend, pipeline) = match self {
            Tier::VaapiDma => (EncoderMode::Hardware, "h264", "vaapi", "vaapi-dmabuf"),
            Tier::VaapiCpu => (EncoderMode::Hardware, "h264", "vaapi", "vaapi-cpu"),
            Tier::X264 => (EncoderMode::Software, "h264", "x264", "x264-cpu"),
        };
        // bitrate and fps are filled in by the caller, which is the only place that holds the
        // resolved `EncoderConfig`. A tier does not know what it was configured with.
        EncoderReport {
            mode,
            codec: codec.into(),
            backend: backend.into(),
            pipeline: pipeline.into(),
            bitrate_kbps: 0,
            fps: 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_per_backend() {
        use EncoderBackend::*;
        use Tier::*;
        assert_eq!(tiers_for(Auto, true), [VaapiDma, VaapiCpu, X264]);
        assert_eq!(tiers_for(Auto, false), [VaapiCpu, X264]);
        // Hardware must never fall through to software.
        assert_eq!(tiers_for(Hardware, true), [VaapiDma, VaapiCpu]);
        assert_eq!(tiers_for(Hardware, false), [VaapiCpu]);
        assert_eq!(tiers_for(Software, true), [X264]);
    }

    #[test]
    fn downgrade_walks_the_ladder_once() {
        assert_eq!(Tier::VaapiDma.next(), Some(Tier::VaapiCpu));
        assert_eq!(Tier::VaapiCpu.next(), Some(Tier::X264));
        assert_eq!(Tier::X264.next(), None);
    }

    #[test]
    fn only_x264_reports_software() {
        // Invariant #5: the software fallback must be visible to the user.
        assert_eq!(Tier::X264.report().mode, EncoderMode::Software);
        assert_eq!(Tier::VaapiDma.report().mode, EncoderMode::Hardware);
        assert_eq!(Tier::VaapiCpu.report().mode, EncoderMode::Hardware);
    }
}
