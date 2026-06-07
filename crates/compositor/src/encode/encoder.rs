//! The encoder abstraction: one trait both backends implement so the render tick is
//! backend-agnostic.
//!
//! A backend takes one captured frame in `Fourcc::Abgr8888` memory layout — `[R, G, B, A]`
//! per pixel, exactly what [`crate::capture::mem::capture_frame`] returns — and emits an
//! H.264 Annex-B access unit (with SPS/PPS prepended on IDR frames), or `None` if it
//! produced no output for that frame.
//!
//! Implementors: [`crate::encode::x264enc::X264Encoder`] (software) and
//! [`crate::encode::ffmpeg::FfmpegVaapiEncoder`] (VAAPI hardware). A future GStreamer
//! backend (`encode::gst`) implements the same trait, so the call site in
//! [`crate::headless::render_tick`] never changes when the backend is swapped.

/// A video-encode backend. Lives on the single compositor (`calloop`) thread, so it is
/// **not** required to be `Send`.
pub trait VideoEncoder {
    /// Encode one RGBA frame. Returns the Annex-B bytes (SPS/PPS prepended on IDR), or
    /// `None` if the encoder emitted nothing this frame.
    fn encode(&mut self, rgba: &[u8]) -> Option<Vec<u8>>;

    /// Request that the next encoded frame be a forced IDR keyframe. Called when a new
    /// viewer connects or the browser sends RTCP PLI/FIR (picture loss).
    fn force_idr_next(&mut self);
}
