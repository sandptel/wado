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
    headless::{self, FPS, HEIGHT, WIDTH},
    sink::{file::FileSink, FrameSink},
    Wado,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all("captures")?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs();
    let h264_path = format!("captures/wado_{}.h264", ts);

    eprintln!("[capture_to_disk] resolution={}x{}  fps={}", WIDTH, HEIGHT, FPS);
    eprintln!("[capture_to_disk] H.264 output : {}", h264_path);
    eprintln!("[capture_to_disk] PPM snapshots: captures/snap_{}_<n>.ppm (every 5 s)", ts);
    eprintln!("[capture_to_disk] Ctrl-C to stop");
    eprintln!();

    let mut event_loop: EventLoop<Wado> = EventLoop::try_new()?;
    let display: Display<Wado> = Display::new()?;
    let mut state = Wado::new(&mut event_loop, display);

    headless::init_headless(&mut event_loop, &mut state)?;
    eprintln!("[capture_to_disk] compositor ready, socket: {:?}", state.socket_name);

    // Grab a fresh set of SPS+PPS for the file (init_headless already sent them
    // to the UDP sink; we need them again for the new FileSink).
    let headers = state.encoder.as_mut().unwrap().headers();
    let mut file_sink = FileSink::create(&h264_path)?;
    file_sink.send(&headers);
    state.frame_sink = Some(Box::new(CountingSink::new(file_sink)));

    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };
    std::process::Command::new("weston-terminal").spawn().ok();

    // The idle callback fires between calloop event batches — close enough to
    // a periodic timer for snapshot purposes.
    let buf_size: Size<i32, Buffer> = (WIDTH as i32, HEIGHT as i32).into();
    let mut last_snap = Instant::now();
    let mut snap_n = 0u32;

    event_loop.run(None, &mut state, move |state| {
        if last_snap.elapsed() < Duration::from_secs(5) {
            return;
        }
        last_snap = Instant::now();
        snap_n += 1;

        // Rebind the renderbuffer to read back the last rendered frame.
        // By this point render_tick has already dropped its GlesTarget, so
        // the renderbuffer is free to bind again.
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
/// PPM is a trivial format: ASCII header + packed RGB bytes — no crate needed.
fn save_ppm(abgr: &[u8], width: usize, height: usize, path: &str) {
    let Ok(mut f) = fs::File::create(path) else {
        eprintln!("[snap] could not create {path}");
        return;
    };
    let _ = write!(f, "P6\n{} {}\n255\n", width, height);
    // ABGR layout: [A=0, B=1, G=2, R=3] per pixel
    let mut rgb = Vec::with_capacity(width * height * 3);
    for px in abgr.chunks_exact(4) {
        rgb.push(px[3]); // R
        rgb.push(px[2]); // G
        rgb.push(px[1]); // B
    }
    let _ = f.write_all(&rgb);
    eprintln!("[snap] saved {path}");
}

/// Wraps FileSink and logs the first frame + periodic progress.
struct CountingSink {
    inner: FileSink,
    count: u64,
}

impl CountingSink {
    fn new(inner: FileSink) -> Self {
        Self { inner, count: 0 }
    }
}

impl FrameSink for CountingSink {
    fn send(&mut self, nal_data: &[u8]) {
        self.count += 1;
        if self.count == 1 {
            eprintln!("[capture_to_disk] first frame written ({} B)", nal_data.len());
        } else if self.count % 300 == 0 {
            // Every ~5 s at 60fps
            eprintln!("[capture_to_disk] frame {}", self.count);
        }
        self.inner.send(nal_data);
    }
}
