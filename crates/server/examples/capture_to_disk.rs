/// Full compositor that writes to disk instead of UDP.
///
/// Saves an H.264 file and PPM snapshots to captures/ (gitignored).
/// Use this when you want a permanent recording, or when no ffplay is handy.
///
/// Usage:
///   cargo run --example capture_to_disk
///   # Ctrl-C to stop
///   ffplay -f h264 captures/wado_<timestamp>.h264
///   # Open captures/snap_<timestamp>_<n>.ppm in any image viewer for stills
///
/// Snapshot interval: every 5 seconds of real time.
/// PPM format: plain RGB, no extra dependencies, opens in GIMP / eog / feh.
use std::{
    fs,
    io::Write,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use smithay::{
    backend::renderer::Bind,
    reexports::{calloop::EventLoop, wayland_server::Display},
    utils::{Buffer, Size},
};
use wado::{
    capture::mem::capture_frame,
    conf::{OutputConfig, SinkTarget, WadoConfig},
    headless::{self, FPS, HEIGHT, WIDTH},
    Wado,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("captures")?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let h264_path = format!("captures/wado_{}.h264", ts);

    eprintln!("[capture_to_disk] resolution={}x{}  fps={}", WIDTH, HEIGHT, FPS);
    eprintln!("[capture_to_disk] H.264 output : {}", h264_path);
    eprintln!("[capture_to_disk] PPM snapshots: captures/snap_{}_<n>.ppm (every 5 s)", ts);
    eprintln!("[capture_to_disk] Ctrl-C to stop");
    eprintln!();

    let mut event_loop: EventLoop<Wado> = EventLoop::try_new()?;
    let display: Display<Wado> = Display::new()?;
    let mut state = Wado::new(&mut event_loop, display);

    // Override the sink to write directly to a file; all other params stay at default.
    let config = WadoConfig {
        output: OutputConfig {
            sink: SinkTarget::File(h264_path.clone()),
            log_encode_stats: false,
        },
        ..WadoConfig::default()
    };
    headless::init_headless(&mut state, &config)?;
    eprintln!("[capture_to_disk] compositor ready, socket: {:?}", state.socket_name);

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };
    std::process::Command::new("weston-terminal").spawn().ok();

    let buf_size: Size<i32, Buffer> = (WIDTH as i32, HEIGHT as i32).into();
    let mut last_snap = Instant::now();
    let mut snap_n = 0u32;

    event_loop.run(None, &mut state, move |state| {
        if last_snap.elapsed() < Duration::from_secs(5) {
            return;
        }
        last_snap = Instant::now();
        snap_n += 1;

        let (Some(renderer), Some(renderbuffer)) =
            (state.renderer.as_mut(), state.renderbuffer.as_mut())
        else {
            return;
        };

        let fb = match renderer.bind(renderbuffer) {
            Ok(fb) => fb,
            Err(e) => {
                eprintln!("[snap] bind error: {e}");
                return;
            }
        };

        match capture_frame(renderer, &fb, buf_size) {
            Ok(abgr) => {
                let path = format!("captures/snap_{}_{}.ppm", ts, snap_n);
                save_ppm(&abgr, WIDTH as usize, HEIGHT as usize, &path);
            }
            Err(e) => eprintln!("[snap] capture_frame error: {e}"),
        }
    })?;

    Ok(())
}

/// Write a PPM (P6) image from ABGR8888 bytes.
fn save_ppm(abgr: &[u8], width: usize, height: usize, path: &str) {
    let Ok(mut f) = fs::File::create(path) else {
        eprintln!("[snap] could not create {path}");
        return;
    };
    let _ = write!(f, "P6\n{} {}\n255\n", width, height);
    // Fourcc::Abgr8888 memory layout on LE: [R=0, G=1, B=2, A=3] (GL_RGBA order)
    let mut rgb = Vec::with_capacity(width * height * 3);
    for px in abgr.chunks_exact(4) {
        rgb.push(px[0]); // R
        rgb.push(px[1]); // G
        rgb.push(px[2]); // B
    }
    let _ = f.write_all(&rgb);
    eprintln!("[snap] saved {path}");
}

