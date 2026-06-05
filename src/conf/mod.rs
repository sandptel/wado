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

/// Where encoded H.264 frames are delivered.
#[derive(Debug, Clone)]
pub enum SinkTarget {
    /// Raw Annex-B bytes to a UDP socket. Test with:
    ///   ffplay -f h264 -i udp://127.0.0.1:5555
    Udp(String),
    /// Raw Annex-B bytes appended to a file.
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
                sink: SinkTarget::Udp("127.0.0.1:5555".to_string()),
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
            SinkTarget::Udp(addr) => eprintln!("[wado] config: sink=udp://{}", addr),
            SinkTarget::File(path) => eprintln!("[wado] config: sink=file://{}", path),
        }
    }
}
