//! The encoder abstraction: one trait both backends implement so the render tick is
//! backend-agnostic.
//!
//! A backend takes one captured [`Frame`] — CPU `Rgba` bytes (`Fourcc::Abgr8888`,
//! `[R, G, B, A]` per pixel) or a borrowed GPU `Dma` buffer — and emits an H.264 Annex-B
//! access unit (with SPS/PPS prepended on IDR frames), or `None` if it produced no output
//! for that frame. Returning `Result` lets the render tick downgrade the pipeline one tier
//! on a hard failure instead of erroring every frame (the runtime fallback).
//!
//! Implementors: [`crate::encode::x264enc::X264Encoder`] (software, CPU only) and
//! [`crate::encode::ffmpeg::FfmpegVaapiEncoder`] (VAAPI hardware, CPU-upload or zero-copy
//! DMA-BUF). A future GStreamer backend (`encode::gst`) implements the same trait, so the
//! call site in [`crate::headless::render_tick`] never changes when the backend is swapped.

use crate::capture::Frame;

/// A video-encode backend. Lives on the single compositor (`calloop`) thread, so it is
/// **not** required to be `Send`.
pub trait VideoEncoder {
    /// Encode one captured [`Frame`]. Returns the Annex-B bytes (SPS/PPS prepended on IDR),
    /// `Ok(None)` if the encoder emitted nothing this frame, or `Err` on a hard failure
    /// (a backend that can't consume the given `Frame` kind also returns `Err`).
    fn submit(&mut self, frame: Frame<'_>) -> crate::Result<Option<Vec<u8>>>;

    /// Request that the next encoded frame be a forced IDR keyframe. Called when a new
    /// viewer connects or the browser sends RTCP PLI/FIR (picture loss).
    fn force_idr_next(&mut self);
}
