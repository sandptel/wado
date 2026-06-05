/// Full compositor with per-second diagnostic stats piped to stderr.
///
/// Standalone, eager diagnostic harness: brings up a full compositor that records
/// H.264 to a file (captures/wado.h264) and logs per-second encode stats. The live
/// WebRTC streaming path is the main binary + the `website` control plane; this
/// example exists purely to profile render/encode throughput without a viewer.
///
/// Usage:
///   cargo run --example view_compositor
///   ffplay -f h264 captures/wado.h264   # to inspect the recording afterwards
///
/// What to look for:
///   [diag] lines show fps≈60, NAL size in the hundreds of bytes per frame.
///   If fps drops well below 60, the render or encode step is the bottleneck.
use std::time::{Duration, Instant};

use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use wado::{
    Wado,
    conf::WadoConfig,
    headless::{self, FPS, HEIGHT, WIDTH},
    sink::FrameSink,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    eprintln!("[view_compositor] resolution={}x{}  fps={}", WIDTH, HEIGHT, FPS);
    eprintln!("[view_compositor] recording to captures/wado.h264 (inspect with ffplay)");
    eprintln!();

    std::fs::create_dir_all("captures")?;

    let mut event_loop: EventLoop<Wado> = EventLoop::try_new()?;
    let display: Display<Wado> = Display::new()?;
    let mut state = Wado::new(&mut event_loop, display);

    eprintln!("[view_compositor] Wayland socket: {:?}", state.socket_name);

    headless::init_headless(&mut state, &WadoConfig::default())?;
    eprintln!("[view_compositor] EGL context, headless output, x264 encoder, file sink — OK");

    // Inject the diagnostic wrapper around the file sink.
    let inner = state
        .frame_sink
        .take()
        .expect("init_headless must populate frame_sink");
    state.frame_sink = Some(Box::new(DiagnosticSink::new(inner)));

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };
    eprintln!("[view_compositor] Spawning weston-terminal...");
    std::process::Command::new("weston-terminal").spawn().ok();

    event_loop.run(None, &mut state, move |_| {})?;
    Ok(())
}

/// Wraps any FrameSink and logs per-second stats to stderr.
/// Forwards every NAL payload to the inner sink unchanged.
struct DiagnosticSink {
    inner: Box<dyn FrameSink>,
    frame_count: u64,
    bytes_sent: u64,
    window_frames: u64,
    start: Instant,
    last_report: Instant,
}

impl DiagnosticSink {
    fn new(inner: Box<dyn FrameSink>) -> Self {
        let now = Instant::now();
        Self {
            inner,
            frame_count: 0,
            bytes_sent: 0,
            window_frames: 0,
            start: now,
            last_report: now,
        }
    }
}

impl FrameSink for DiagnosticSink {
    fn send(&mut self, nal_data: &[u8]) {
        self.frame_count += 1;
        self.window_frames += 1;
        self.bytes_sent += nal_data.len() as u64;

        let elapsed = self.last_report.elapsed();
        if elapsed >= Duration::from_secs(1) {
            let fps = self.window_frames as f64 / elapsed.as_secs_f64();
            let uptime = self.start.elapsed().as_secs();
            eprintln!(
                "[diag] uptime={:4}s  frame={:6}  fps≈{:5.1}  nal={:5}B  sent_total={:.1}KB",
                uptime,
                self.frame_count,
                fps,
                nal_data.len(),
                self.bytes_sent as f64 / 1024.0,
            );
            self.window_frames = 0;
            self.last_report = Instant::now();
        }

        self.inner.send(nal_data);
    }
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }
}
