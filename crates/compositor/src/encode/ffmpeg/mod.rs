//! Hardware H.264 encode via **ffmpeg + VAAPI** (`h264_vaapi`).
//!
//! Per frame (Step 1, host-memory capture): the captured RGBA is converted on the CPU to
//! NV12 ([`convert::rgba_to_nv12`]), uploaded into a VAAPI surface
//! (`av_hwframe_transfer_data`), and encoded by `h264_vaapi`. Output is **Annex-B** with
//! SPS/PPS inlined before every IDR (we do NOT set `AV_CODEC_FLAG_GLOBAL_HEADER`), so a
//! late-joining viewer syncs on the next keyframe — same contract as the x264 path.
//!
//! The encoder is configured for **low latency** (invariant #7): CBR, `max_b_frames = 0`,
//! short GOP. Step 2 will replace the CPU convert+upload with a DMA-BUF import.
//!
//! All ffmpeg FFI lives under this module (`mod` + [`hwcontext`] + [`convert`] + [`probe`]),
//! so the rest of the compositor only ever sees the [`VideoEncoder`] trait.

pub mod convert;
pub mod hwcontext;
pub mod probe;

use std::ffi::CString;
use std::ptr;
use std::slice;

use ffmpeg_the_third::ffi::{
    AVCodecContext, AVDictionary, AVFrame, AVPictureType, AVPixelFormat, AVRational,
    av_buffer_ref, av_dict_free, av_dict_set, av_frame_alloc, av_frame_free, av_frame_get_buffer,
    av_frame_make_writable, av_hwframe_get_buffer, av_hwframe_transfer_data, av_packet_alloc,
    av_packet_free, av_packet_unref, avcodec_alloc_context3, avcodec_find_encoder_by_name,
    avcodec_free_context, avcodec_open2, avcodec_receive_packet, avcodec_send_frame,
};

use crate::CompositorError;
use crate::encode::encoder::VideoEncoder;
use hwcontext::{DeviceRef, FramesRef, create_nv12_frames, create_vaapi_device};

/// `avcodec_receive_packet` returns this when it needs another input frame — not an error.
const AVERROR_EAGAIN: i32 = -11; // -EAGAIN on Linux
/// `FFERRTAG('E','O','F',' ')` negated — end of stream, also not an error here.
const AVERROR_EOF: i32 = -0x20464F45;

/// A VAAPI H.264 encoder. Owns the GPU device, the NV12 surface pool, and the codec ctx.
pub struct FfmpegVaapiEncoder {
    // Drop order: ctx is freed explicitly in `Drop`; `_frames`/`_device` unref after.
    ctx: *mut AVCodecContext,
    _frames: FramesRef,
    _device: DeviceRef,
    width: usize,
    height: usize,
    pts: i64,
    /// Set by `force_idr_next`; makes the next submitted frame an IDR.
    force_idr: bool,
}

impl FfmpegVaapiEncoder {
    /// Open a VAAPI H.264 encoder on `node` at the given parameters. `width`/`height` must
    /// be even. Returns an error (rather than panicking) if any VAAPI/codec step fails, so
    /// the probe and the `Auto` fallback can recover.
    pub fn new(
        node: &str,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_kbps: u32,
        keyframe_interval: u32,
    ) -> crate::Result<Self> {
        let w = width as i32;
        let h = height as i32;
        let device = create_vaapi_device(node)?;
        let frames = create_nv12_frames(&device, w, h, 4)?;

        unsafe {
            let name = CString::new("h264_vaapi").unwrap();
            let codec = avcodec_find_encoder_by_name(name.as_ptr());
            if codec.is_null() {
                return Err(CompositorError::Encoder("h264_vaapi encoder not found".into()));
            }
            let ctx = avcodec_alloc_context3(codec);
            if ctx.is_null() {
                return Err(CompositorError::Encoder("avcodec_alloc_context3 failed".into()));
            }

            (*ctx).width = w;
            (*ctx).height = h;
            (*ctx).time_base = AVRational { num: 1, den: fps.max(1) as i32 };
            (*ctx).framerate = AVRational { num: fps.max(1) as i32, den: 1 };
            (*ctx).pix_fmt = AVPixelFormat::VAAPI;
            (*ctx).max_b_frames = 0; // invariant #7: no reordering latency
            (*ctx).gop_size = keyframe_interval.max(1) as i32;
            let bitrate = (bitrate_kbps as i64) * 1000;
            (*ctx).bit_rate = bitrate;
            (*ctx).rc_max_rate = bitrate; // CBR: clamp the rate control window
            (*ctx).rc_min_rate = bitrate;
            (*ctx).rc_buffer_size = bitrate as i32; // ~1 s VBV
            // The encoder draws output surfaces from our NV12 VAAPI pool.
            (*ctx).hw_frames_ctx = av_buffer_ref(frames.0);

            // h264_vaapi private options: CBR rate control, minimal pipelining for latency.
            let mut opts: *mut AVDictionary = ptr::null_mut();
            set_opt(&mut opts, "rc_mode", "CBR");
            set_opt(&mut opts, "async_depth", "1");

            let ret = avcodec_open2(ctx, codec, &mut opts);
            av_dict_free(&mut opts);
            if ret < 0 {
                let mut c = ctx;
                avcodec_free_context(&mut c);
                return Err(CompositorError::Encoder(format!("avcodec_open2(h264_vaapi) failed: {ret}")));
            }

            Ok(Self {
                ctx,
                _frames: frames,
                _device: device,
                width: width as usize,
                height: height as usize,
                pts: 0,
                force_idr: false,
            })
        }
    }

    /// Encode one RGBA frame. `Ok(None)` means the encoder buffered the frame and emitted
    /// nothing yet (normal); `Err` means a fatal VAAPI/codec failure (drives the probe).
    pub fn encode_frame(&mut self, rgba: &[u8]) -> crate::Result<Option<Vec<u8>>> {
        let w = self.width;
        let h = self.height;
        let nv12 = convert::rgba_to_nv12(rgba, w, h);

        unsafe {
            // ── Software NV12 frame holding the converted pixels ──────────────────────
            let sw = av_frame_alloc();
            if sw.is_null() {
                return Err(CompositorError::Encoder("av_frame_alloc(sw) failed".into()));
            }
            (*sw).format = AVPixelFormat::NV12.0;
            (*sw).width = w as i32;
            (*sw).height = h as i32;
            if av_frame_get_buffer(sw, 32) < 0 {
                free_frame(sw);
                return Err(CompositorError::Encoder("av_frame_get_buffer(sw) failed".into()));
            }
            if av_frame_make_writable(sw) < 0 {
                free_frame(sw);
                return Err(CompositorError::Encoder("av_frame_make_writable failed".into()));
            }
            // Y plane (stride width), then interleaved UV plane (stride width).
            for row in 0..h {
                let dst = (*sw).data[0].add(row * (*sw).linesize[0] as usize);
                ptr::copy_nonoverlapping(nv12.as_ptr().add(row * w), dst, w);
            }
            let uv_off = w * h;
            for row in 0..(h / 2) {
                let dst = (*sw).data[1].add(row * (*sw).linesize[1] as usize);
                ptr::copy_nonoverlapping(nv12.as_ptr().add(uv_off + row * w), dst, w);
            }

            // ── Upload into a VAAPI surface ───────────────────────────────────────────
            let hw = av_frame_alloc();
            if hw.is_null() {
                free_frame(sw);
                return Err(CompositorError::Encoder("av_frame_alloc(hw) failed".into()));
            }
            if av_hwframe_get_buffer(self._frames.0, hw, 0) < 0 {
                free_frame(sw);
                free_frame(hw);
                return Err(CompositorError::Encoder("av_hwframe_get_buffer failed".into()));
            }
            if av_hwframe_transfer_data(hw, sw, 0) < 0 {
                free_frame(sw);
                free_frame(hw);
                return Err(CompositorError::Encoder("av_hwframe_transfer_data failed".into()));
            }
            (*hw).pts = self.pts;
            self.pts += 1;
            if self.force_idr {
                self.force_idr = false;
                (*hw).pict_type = AVPictureType::I;
            }

            // ── Submit + drain ────────────────────────────────────────────────────────
            let send = avcodec_send_frame(self.ctx, hw);
            free_frame(sw);
            free_frame(hw);
            if send < 0 {
                return Err(CompositorError::Encoder(format!("avcodec_send_frame failed: {send}")));
            }

            let mut out = Vec::new();
            let pkt = av_packet_alloc();
            if pkt.is_null() {
                return Err(CompositorError::Encoder("av_packet_alloc failed".into()));
            }
            loop {
                let ret = avcodec_receive_packet(self.ctx, pkt);
                if ret == 0 {
                    let data = slice::from_raw_parts((*pkt).data, (*pkt).size as usize);
                    out.extend_from_slice(data);
                    av_packet_unref(pkt);
                } else {
                    if ret != AVERROR_EAGAIN && ret != AVERROR_EOF {
                        tracing::warn!("avcodec_receive_packet error: {ret}");
                    }
                    break;
                }
            }
            let mut p = pkt;
            av_packet_free(&mut p);

            Ok(if out.is_empty() { None } else { Some(out) })
        }
    }
}

impl VideoEncoder for FfmpegVaapiEncoder {
    fn encode(&mut self, rgba: &[u8]) -> Option<Vec<u8>> {
        match self.encode_frame(rgba) {
            Ok(out) => out,
            Err(e) => {
                tracing::warn!("vaapi encode failed: {e}");
                None
            }
        }
    }

    fn force_idr_next(&mut self) {
        self.force_idr = true;
    }
}

impl Drop for FfmpegVaapiEncoder {
    fn drop(&mut self) {
        unsafe {
            if !self.ctx.is_null() {
                avcodec_free_context(&mut self.ctx);
            }
        }
    }
}

/// Set a string option in an `AVDictionary` (best-effort; bad keys are ignored by ffmpeg).
fn set_opt(opts: &mut *mut AVDictionary, key: &str, val: &str) {
    if let (Ok(k), Ok(v)) = (CString::new(key), CString::new(val)) {
        unsafe { av_dict_set(opts, k.as_ptr(), v.as_ptr(), 0) };
    }
}

/// Free an `AVFrame` (helper to keep the unsafe error paths terse).
unsafe fn free_frame(frame: *mut AVFrame) {
    let mut f = frame;
    unsafe { av_frame_free(&mut f) };
}
