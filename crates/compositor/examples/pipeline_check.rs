/// Standalone encode pipeline test — no EGL, no Wayland, no event loop.
///
/// Exercises X264Encoder + FileSink with a synthetic ABGR test pattern.
/// Use this first when debugging: if this fails, the problem is in the
/// encode/sink stage, not in the compositor/EGL stage.
///
/// Usage:
///   cargo run --example pipeline_check
///   ffplay -f h264 captures/pipeline_check.h264
use std::{fs, time::Instant};

use wado_compositor::{
    conf::{KeyframeMode, Preset},
    encode::x264enc::X264Encoder,
    sink::{file::FileSink, FrameSink},
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const FPS: u32 = 60;
const FRAMES: u32 = 120;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "[pipeline_check] {}x{} @ {} fps  —  {} frames (no EGL/Wayland needed)",
        WIDTH, HEIGHT, FPS, FRAMES
    );
    eprintln!();

    // ── Step 1: build encoder ─────────────────────────────────────────────────
    let t = Instant::now();
    let mut encoder =
        X264Encoder::new(WIDTH, HEIGHT, FPS, 4000, 30, KeyframeMode::Periodic, Preset::Ultrafast)
        .inspect_err(|e| eprintln!("[FAIL] X264Encoder::new: {e}"))?;
    eprintln!("[1/5] X264Encoder::new  OK  ({:.1} ms)", ms(t));

    // ── Step 2: SPS + PPS headers ─────────────────────────────────────────────
    let headers = encoder.headers();
    if headers.is_empty() {
        eprintln!("[FAIL] encoder.headers() returned empty — broken encoder init");
        std::process::exit(1);
    }
    eprintln!("[2/5] headers           OK  ({} bytes — SPS+PPS)", headers.len());

    // ── Step 3: open output file ──────────────────────────────────────────────
    fs::create_dir_all("captures")?;
    let out_path = "captures/pipeline_check.h264";
    let mut sink = FileSink::create(out_path)
        .inspect_err(|e| eprintln!("[FAIL] FileSink::create: {e}"))?;
    sink.send(&headers);
    eprintln!("[3/5] FileSink          OK  ({})", out_path);

    // ── Step 4: encode FRAMES synthetic frames ────────────────────────────────
    eprintln!("[4/5] encoding {} frames...", FRAMES);
    let mut min_ms = f64::MAX;
    let mut max_ms = 0.0f64;
    let mut sum_ms = 0.0f64;
    let mut encoded = 0u32;
    let mut skipped = 0u32;
    let mut total_bytes = 0u64;

    for i in 0..FRAMES {
        let frame = make_test_pattern(WIDTH, HEIGHT, i);
        let t = Instant::now();
        let result = encoder.encode_rgba(&frame);
        let dur = ms(t);

        match result {
            Some(nal) => {
                total_bytes += nal.len() as u64;
                sink.send(&nal);
                encoded += 1;
                min_ms = min_ms.min(dur);
                max_ms = max_ms.max(dur);
                sum_ms += dur;
                // Show the first few and every GOP boundary so the log isn't huge.
                if i < 4 || i % 30 == 0 {
                    eprintln!("    frame {:3}  {:.2} ms  {} B", i, dur, nal.len());
                }
            }
            None => {
                skipped += 1;
                eprintln!("    frame {:3}  skipped (encoder returned None — may be normal for B-frame delay)", i);
            }
        }
    }

    // ── Step 5: report ────────────────────────────────────────────────────────
    eprintln!();
    eprintln!("[5/5] results:");
    eprintln!("  encoded / total   : {} / {}", encoded, FRAMES);
    if skipped > 0 {
        eprintln!("  skipped           : {}  (zero_latency=true so this should be 0)", skipped);
    }
    if encoded > 0 {
        let mean = sum_ms / encoded as f64;
        eprintln!("  encode latency    : min={:.2}ms  mean={:.2}ms  max={:.2}ms", min_ms, mean, max_ms);
        eprintln!("  encode-only FPS   : {:.1}  (headroom before 60fps budget is exhausted)", 1000.0 / mean);
    }
    eprintln!("  output size       : {:.1} KB", total_bytes as f64 / 1024.0);
    eprintln!();
    eprintln!("[pipeline_check] to verify visually:");
    eprintln!("  ffplay -f h264 {}", out_path);

    Ok(())
}

/// Synthetic ABGR8888 frame: hue-rotating gradient so every frame differs
/// (prevents the encoder from emitting only P-frames with near-zero content).
fn make_test_pattern(width: u32, height: u32, frame: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut data = vec![0u8; w * h * 4];
    // Phase advances 6° per frame at 60fps → full rotation every 60 frames.
    let phase = (frame as f32) * (360.0 / FPS as f32);

    for row in 0..h {
        for col in 0..w {
            let base = (row * w + col) * 4;
            let hue = (phase + col as f32 / w as f32 * 120.0) % 360.0;
            let sat = 0.4 + 0.6 * row as f32 / h as f32;
            let (r, g, b) = hsv_to_rgb(hue, sat, 1.0);
            // Fourcc::Abgr8888 on LE = [R,G,B,A] in memory (GL_RGBA order)
            data[base]     = r;
            data[base + 1] = g;
            data[base + 2] = b;
            data[base + 3] = 255; // A
        }
    }
    data
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

#[inline]
fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
