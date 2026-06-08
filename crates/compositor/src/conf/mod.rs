//! Server-side runtime configuration. The client-facing wire types
//! ([`SessionConfig`], [`Quality`]) live in the `wado-protocol` crate and are
//! re-exported here; this module owns the x264-coupled encoder mapping.

pub use wado_protocol::{EncoderBackend, KeyframeMode, Quality, SessionConfig};
pub use x264::Preset;

pub const DEFAULT_WIDTH: u32 = 1280;
pub const DEFAULT_HEIGHT: u32 = 720;
pub const DEFAULT_FPS: u32 = 60;

/// Top-level runtime configuration for a wado session.
#[derive(Debug, Clone)]
pub struct WadoConfig {
    pub encoder: EncoderConfig,
    pub output: OutputConfig,
}

/// Encoder and capture parameters.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// CBR target in kbps.
    pub bitrate_kbps: u32,
    /// Frames between IDR keyframes **when `keyframe_mode` is `Periodic`**. A late-joining
    /// client syncs within this many frames. At 60fps, 30 → 0.5 s. Ignored in `OnDemand`
    /// mode (which uses a huge GOP and IDRs only on connect/PLI).
    pub keyframe_interval: u32,
    /// x264 preset. **Software-path only** — the hardware (VAAPI) backend derives its rate
    /// control from `bitrate_kbps` / `fps` / `keyframe_interval` and ignores this.
    pub preset: Preset,
    /// Which encode backend to use (hardware/software/auto). Resolved by
    /// [`crate::encode::select::build_encoder`].
    pub backend: EncoderBackend,
    /// Master low-latency lock (informational/logging; the keyframe effect is already baked
    /// into `keyframe_mode`). Software (x264) always runs low-latency regardless.
    pub zero_latency: bool,
    /// Effective keyframe strategy after applying the `zero_latency` lock.
    pub keyframe_mode: KeyframeMode,
}

/// Where encoded H.264 frames are delivered (for standalone examples).
///
/// The live path is driven by the `website` control plane, which feeds frames
/// through a `ChannelSink` into the WebRTC track — it does not go through this enum.
#[derive(Debug, Clone)]
pub enum SinkTarget {
    /// Raw Annex-B bytes appended to a file. Debug / recording path.
    File(String),
}

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub sink: SinkTarget,
    /// Log per-frame encode time to stderr.
    pub log_encode_stats: bool,
}

impl Default for WadoConfig {
    fn default() -> Self {
        Self {
            encoder: EncoderConfig {
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
                fps: DEFAULT_FPS,
                bitrate_kbps: 4000,
                keyframe_interval: 30,
                preset: Preset::Ultrafast,
                backend: EncoderBackend::Auto,
                zero_latency: false,
                keyframe_mode: KeyframeMode::Periodic,
            },
            output: OutputConfig {
                sink: SinkTarget::File("captures/wado.h264".to_string()),
                log_encode_stats: false,
            },
        }
    }
}

impl WadoConfig {
    pub fn print_summary(&self) {
        let e = &self.encoder;
        eprintln!(
            "[wado] config: {}x{} @ {} fps  bitrate={}kbps  keyframe_interval={}  preset={:?}",
            e.width, e.height, e.fps, e.bitrate_kbps, e.keyframe_interval, e.preset
        );
        match &self.output.sink {
            SinkTarget::File(path) => eprintln!("[wado] config: sink=file://{}", path),
        }
    }
}

/// Resolve a client-supplied [`SessionConfig`]'s quality preset and any advanced
/// overrides into concrete encoder parameters.
///
/// A free function (not an inherent method) because `SessionConfig` is defined in
/// the `wado-protocol` crate — the orphan rule forbids adding inherent impls here.
pub fn to_encoder_config(config: &SessionConfig) -> EncoderConfig {
    let fps = config.fps.max(1);
    let (bitrate_kbps, default_preset, default_kf) = match config.quality {
        Quality::Reactivity => (2000, Preset::Ultrafast, fps), // ~1 s GOP
        Quality::Balanced => (4000, Preset::Ultrafast, fps * 2),
        Quality::Quality => (8000, Preset::Veryfast, fps * 2),
        Quality::Custom { bitrate_kbps } => (bitrate_kbps, Preset::Ultrafast, fps * 2),
    };
    // The zero-latency master lock forces on-demand keyframes; otherwise honour the knob.
    let keyframe_mode = if config.tuning.zero_latency {
        KeyframeMode::OnDemand
    } else {
        config.tuning.keyframe_mode
    };
    EncoderConfig {
        width: config.width,
        height: config.height,
        fps,
        bitrate_kbps,
        keyframe_interval: config.keyframe_interval.unwrap_or(default_kf),
        preset: config.preset.as_deref().map(parse_preset).unwrap_or(default_preset),
        backend: config.encoder.backend,
        zero_latency: config.tuning.zero_latency,
        keyframe_mode,
    }
}

/// Parse an x264 preset name; unknown names fall back to `Ultrafast`.
pub fn parse_preset(name: &str) -> Preset {
    match name.to_ascii_lowercase().as_str() {
        "ultrafast" => Preset::Ultrafast,
        "superfast" => Preset::Superfast,
        "veryfast" => Preset::Veryfast,
        "faster" => Preset::Faster,
        "fast" => Preset::Fast,
        "medium" => Preset::Medium,
        _ => Preset::Ultrafast,
    }
}
