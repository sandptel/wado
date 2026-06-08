//! Video encoders. [`encoder::VideoEncoder`] is the backend-agnostic trait; [`select`]
//! picks a concrete backend per the session's [`wado_protocol::EncoderBackend`]
//! preference (probing hardware, falling back to software). Backends:
//! [`x264enc`] (software) and [`ffmpeg`] (VAAPI hardware).

pub mod encoder;
pub mod ffmpeg;
pub mod select;
pub mod stall_watchdog;
pub mod x264enc;

// ── Shared keyframe constants ──────────────────────────────────────────────────────────
// Used by both the VAAPI and x264 encoders for the `KeyframeMode::OnDemand` GOP.
// The on-demand mode avoids periodic IDR spikes (invariant #7) by using a long GOP,
// but a pathologically large GOP (e.g. fps×3600 = 432000 @120fps) can cause the AMD
// VAAPI encoder to stall silently under rapid frame-delta bursts. These constants keep
// the GOP "effectively infinite" for reconnect/PLI purposes while bounding the P-chain.

/// Duration of the on-demand GOP in seconds. 10 s means no periodic IDR for ~10 s —
/// PLI/connect IDRs come far more often in practice, so the viewer never waits this long.
pub const ON_DEMAND_GOP_SECS: u32 = 10;
/// Hard ceiling on `gop_size` regardless of frame rate. At 120fps, 10 s = 1200 frames;
/// at 60fps it is 600 frames. Prevents the encoder from building an unbounded P-chain.
pub const ON_DEMAND_GOP_MAX: u32 = 1200;
