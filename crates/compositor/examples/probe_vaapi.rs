//! Standalone VAAPI hardware-encoder check — no EGL, no Wayland, no event loop.
//!
//! Runs the open-and-cache probe, then opens a real-size `h264_vaapi` encoder and pushes
//! a few synthetic RGBA frames through it, reporting how many Annex-B bytes came back.
//! Use this to verify the hardware path in isolation (the server's `Auto` selection runs
//! the same probe).
//!
//! Usage:
//!   cargo run -p wado-compositor --example probe_vaapi

use std::borrow::Cow;

use wado_compositor::capture::Frame;
use wado_compositor::conf::KeyframeMode;
use wado_compositor::encode::encoder::VideoEncoder;
use wado_compositor::encode::ffmpeg::{FfmpegVaapiEncoder, hwcontext::first_render_node, probe};

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    println!("render node: {:?}", first_render_node());
    println!("vaapi_available() (cached probe): {}", probe::vaapi_available());

    let node = match first_render_node() {
        Some(n) => n,
        None => {
            println!("no render node — cannot test hardware encode");
            return;
        }
    };

    let (w, h) = (1280u32, 720u32);
    let mut enc = match FfmpegVaapiEncoder::new(&node, w, h, 60, 4000, 30, KeyframeMode::OnDemand) {
        Ok(e) => e,
        Err(e) => {
            println!("FAILED to open VAAPI encoder at {w}x{h}: {e}");
            return;
        }
    };
    println!("opened h264_vaapi at {w}x{h}");

    // A simple moving gradient so the encoder has real content (and motion).
    let mut total = 0usize;
    for f in 0..10u32 {
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = ((x + f * 8) & 0xff) as u8; // R
                rgba[i + 1] = ((y + f * 4) & 0xff) as u8; // G
                rgba[i + 2] = (f * 16) as u8; // B
                rgba[i + 3] = 0xff; // A
            }
        }
        if f == 5 {
            enc.force_idr_next();
        }
        match enc.submit(Frame::Rgba(Cow::Borrowed(&rgba))) {
            Ok(Some(nal)) => {
                total += nal.len();
                println!("frame {f}: {} bytes", nal.len());
            }
            Ok(None) => println!("frame {f}: (buffered, no output yet)"),
            Err(e) => {
                println!("frame {f}: ENCODE ERROR: {e}");
                return;
            }
        }
    }
    println!("done — {total} total Annex-B bytes over 10 frames");
}
