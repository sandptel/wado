use serde::Deserialize;
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
    /// Maximum (and minimum with scenecut disabled) frames between IDR keyframes.
    /// A late-joining client syncs within this many frames. At 60fps, 30 → 0.5 s.
    pub keyframe_interval: u32,
    pub preset: Preset,
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

/// Image-quality preset chosen by the web client (RustDesk's model). Maps to a
/// bitrate + x264 preset + keyframe interval via [`SessionConfig::to_encoder_config`].
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Lowest latency: low bitrate, fastest preset, short GOP.
    Reactivity,
    /// Middle ground (default).
    Balanced,
    /// Higher bitrate / better image at some CPU cost.
    Quality,
    /// Explicit CBR target in kbps.
    Custom { bitrate_kbps: u32 },
}

/// One session's configuration, as sent by the web client (`POST /session/start`).
#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub quality: Quality,
    /// Free-form command to launch inside the session (program + args, space-split).
    pub command: String,
    /// Advanced override: x264 preset name ("ultrafast".."veryfast"). Falls back to
    /// the quality preset's default when absent.
    #[serde(default)]
    pub preset: Option<String>,
    /// Advanced override: frames between IDR keyframes. Falls back to the quality
    /// preset's default when absent.
    #[serde(default)]
    pub keyframe_interval: Option<u32>,
}

impl SessionConfig {
    /// Resolve the quality preset and any advanced overrides into concrete encoder
    /// parameters.
    pub fn to_encoder_config(&self) -> EncoderConfig {
        let fps = self.fps.max(1);
        let (bitrate_kbps, default_preset, default_kf) = match self.quality {
            Quality::Reactivity => (2000, Preset::Ultrafast, fps), // ~1 s GOP
            Quality::Balanced => (4000, Preset::Ultrafast, fps * 2),
            Quality::Quality => (8000, Preset::Veryfast, fps * 2),
            Quality::Custom { bitrate_kbps } => (bitrate_kbps, Preset::Ultrafast, fps * 2),
        };
        EncoderConfig {
            width: self.width,
            height: self.height,
            fps,
            bitrate_kbps,
            keyframe_interval: self.keyframe_interval.unwrap_or(default_kf),
            preset: self.preset.as_deref().map(parse_preset).unwrap_or(default_preset),
        }
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
