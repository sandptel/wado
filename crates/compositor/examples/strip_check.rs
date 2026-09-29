/// Live check for the phone strip's won't-fit handling (M-P S3): a portrait, phone-shaped
/// session in `Placement::Strip`, one app that refuses to go as narrow as a column, and a frame
/// written to disk to look at.
///
/// Usage (the GPU needs the sandbox off):
///   cargo run --release --example strip_check -- [command] [scale]  # default: nautilus 2
///   ffmpeg -f rawvideo -pix_fmt rgba -s 720x1600 -i captures/strip_check.rgba captures/strip_check.png
///
/// Prints each column's draw factor. A factor below 1 is S3 at work; the PNG should show the
/// whole app inside the column rather than cut off at its right edge.
use std::time::{Duration, Instant};

use wado_compositor::{
    conf::{EncoderConfig, Preset},
    headless,
    sink::file::FileSink,
};
use wado_protocol::{EncoderBackend, Placement};

const W: u32 = 720;
const H: u32 = 1600;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "nautilus".into());
    // A higher scale is a narrower column in logical pixels — how to make an app refuse.
    let scale: f32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    std::fs::create_dir_all("captures")?;

    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (mut event_loop, mut state, _handles) = wado_compositor::build(frame_tx)?;
    state.placement = Placement::Strip;

    let ec = EncoderConfig {
        width: W,
        height: H,
        fps: 30,
        bitrate_kbps: 2000,
        keyframe_interval: 60,
        preset: Preset::Ultrafast,
        // Software: the CPU capture path is the one `snapshot_rgba` can read back.
        backend: EncoderBackend::Software,
    };
    let sink = FileSink::create("captures/strip_check.h264")?;
    headless::start_session(&mut state, &ec, scale, Box::new(sink))?;
    headless::launch_command(&mut state, &command);

    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        event_loop.dispatch(Some(Duration::from_millis(16)), &mut state)?;
        state.after_dispatch();
    }

    for c in &state.strip {
        let geo = c.window.geometry();
        println!(
            "column: factor {:.3}, app size {}x{} logical → drawn {:.0} wide in a {} column",
            c.scale,
            geo.size.w,
            geo.size.h,
            f64::from(geo.size.w) * c.scale,
            (W as f32 / scale) as i32,
        );
    }
    match headless::snapshot_rgba(&mut state) {
        Some(px) => {
            std::fs::write("captures/strip_check.rgba", px)?;
            println!("frame: captures/strip_check.rgba ({W}x{H} rgba)");
        }
        None => println!("no frame could be read back"),
    }
    headless::stop_session(&mut state);
    Ok(())
}
