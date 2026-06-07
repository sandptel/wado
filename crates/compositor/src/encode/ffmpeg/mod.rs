//! Hardware H.264 encode via **ffmpeg + VAAPI** (`h264_vaapi`), with two feeds behind one
//! type:
//!
//! - **CPU upload** (`Feed::Cpu`, M3 Step 1): captured RGBA → NV12 on the CPU
//!   ([`convert::rgba_to_nv12`]) → `av_hwframe_transfer_data` into a VAAPI surface → encode.
//! - **Zero-copy DMA-BUF** (`Feed::Dma`, M3 Step 2): the rendered dmabuf is wrapped as a
//!   DRM-PRIME frame ([`dmabuf_import`]) and mapped + colour-converted RGBA→NV12 on the GPU
//!   by a VAAPI VPP filtergraph ([`vpp`]) → encode. No CPU round-trip.
//!
//! Output is **Annex-B** with SPS/PPS inlined before every IDR (no `AV_CODEC_FLAG_GLOBAL_HEADER`),
//! so a late-joining viewer syncs on the next keyframe. Configured for **low latency**
//! (invariant #7): CBR, `max_b_frames = 0`, short GOP. All ffmpeg FFI is isolated under this
//! module so the rest of the compositor only sees the [`VideoEncoder`] trait.

pub mod convert;
pub mod dmabuf_import;
pub mod hwcontext;
pub mod probe;
pub mod vpp;

use std::ffi::CString;
use std::ptr;
use std::slice;

use ffmpeg_the_third::ffi::{
    AVBufferRef, AVCodecContext, AVDictionary, AVFrame, AVPictureType, AVPixelFormat, AVRational,
    av_buffer_ref, av_buffer_unref, av_dict_free, av_dict_set, av_frame_alloc, av_frame_free,
    av_frame_get_buffer, av_frame_make_writable, av_hwframe_get_buffer, av_hwframe_transfer_data,
    av_packet_alloc, av_packet_free, av_packet_unref, avcodec_alloc_context3,
    avcodec_find_encoder_by_name, avcodec_free_context, avcodec_open2, avcodec_receive_packet,
    avcodec_send_frame,
};
use smithay::backend::allocator::dmabuf::Dmabuf;

use crate::CompositorError;
use crate::capture::Frame;
use crate::encode::encoder::VideoEncoder;
use hwcontext::{
    DeviceRef, FramesRef, create_drm_device, create_drm_prime_frames, create_nv12_frames,
    create_vaapi_device, derive_vaapi_device,
};

/// `avcodec_receive_packet` returns this when it needs another input frame — not an error.
pub(super) const AVERROR_EAGAIN: i32 = -11; // -EAGAIN on Linux
/// `FFERRTAG('E','O','F',' ')` negated — end of stream, also not an error here.
pub(super) const AVERROR_EOF: i32 = -0x20464F45;

/// How the encoder gets its NV12 VAAPI surfaces.
enum Feed {
    /// CPU upload: we own the NV12 VAAPI pool and copy into it each frame.
    Cpu { _device: DeviceRef, frames: FramesRef },
    /// Zero-copy: a DRM device + a VPP filtergraph map+convert the dmabuf on the GPU. The
    /// derived VAAPI device and DRM frames pool are kept alive here.
    Dma {
        _drm: DeviceRef,
        _vaapi: DeviceRef,
        drm_frames: FramesRef,
        graph: vpp::FilterGraph,
    },
}

/// A VAAPI H.264 encoder. Owns the codec context and (via [`Feed`]) the GPU contexts.
pub struct FfmpegVaapiEncoder {
    // Drop order: `ctx` is freed explicitly in `Drop`; `feed`'s contexts unref after.
    ctx: *mut AVCodecContext,
    feed: Feed,
    width: usize,
    height: usize,
    pts: i64,
    /// Set by `force_idr_next`; makes the next submitted frame an IDR.
    force_idr: bool,
}

impl FfmpegVaapiEncoder {
    /// Open a CPU-upload VAAPI encoder on `node` (Tier B). `width`/`height` must be even.
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
        let ctx = unsafe { open_h264_vaapi(w, h, fps, bitrate_kbps, keyframe_interval, frames.0)? };
        Ok(Self {
            ctx,
            feed: Feed::Cpu { _device: device, frames },
            width: width as usize,
            height: height as usize,
            pts: 0,
            force_idr: false,
        })
    }

    /// Open a **zero-copy** VAAPI encoder on `node` (Tier A): a DRM device, a VPP filtergraph
    /// that maps DMA-BUFs to VAAPI and converts RGBA→NV12 on the GPU, and an encoder bound to
    /// the graph's NV12 output frames. Errors if any step fails (the probe + ladder recover).
    pub fn new_dma(
        node: &str,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_kbps: u32,
        keyframe_interval: u32,
    ) -> crate::Result<Self> {
        let w = width as i32;
        let h = height as i32;
        let drm = create_drm_device(node)?;
        let vaapi = derive_vaapi_device(&drm)?;
        // DRM-PRIME input frames carry the dmabuf; sw_format RGBA matches DRM ABGR8888.
        let drm_frames = create_drm_prime_frames(&drm, AVPixelFormat::RGBA, w, h)?;
        let graph = vpp::FilterGraph::new(&drm_frames, w, h, fps)?;

        // Bind the encoder to the graph's NV12 VAAPI output frames context.
        let out_frames = graph.output_frames_ctx()?;
        let ctx = unsafe { open_h264_vaapi(w, h, fps, bitrate_kbps, keyframe_interval, out_frames) };
        unsafe {
            let mut f = out_frames;
            av_buffer_unref(&mut f); // the codec ctx took its own ref
        }
        let ctx = ctx?;

        Ok(Self {
            ctx,
            feed: Feed::Dma { _drm: drm, _vaapi: vaapi, drm_frames, graph },
            width: width as usize,
            height: height as usize,
            pts: 0,
            force_idr: false,
        })
    }

    /// CPU-upload feed: RGBA → NV12 (CPU) → upload to a VAAPI surface → encode.
    fn encode_cpu(&mut self, rgba: &[u8]) -> crate::Result<Option<Vec<u8>>> {
        let Feed::Cpu { frames, .. } = &self.feed else {
            return Err(CompositorError::Encoder("encode_cpu on a non-CPU feed".into()));
        };
        let frames_ptr = frames.0;
        let w = self.width;
        let h = self.height;
        let nv12 = convert::rgba_to_nv12(rgba, w, h);

        unsafe {
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
            for row in 0..h {
                let dst = (*sw).data[0].add(row * (*sw).linesize[0] as usize);
                ptr::copy_nonoverlapping(nv12.as_ptr().add(row * w), dst, w);
            }
            let uv_off = w * h;
            for row in 0..(h / 2) {
                let dst = (*sw).data[1].add(row * (*sw).linesize[1] as usize);
                ptr::copy_nonoverlapping(nv12.as_ptr().add(uv_off + row * w), dst, w);
            }

            let hw = av_frame_alloc();
            if hw.is_null() {
                free_frame(sw);
                return Err(CompositorError::Encoder("av_frame_alloc(hw) failed".into()));
            }
            if av_hwframe_get_buffer(frames_ptr, hw, 0) < 0 {
                free_frame(sw);
                free_frame(hw);
                return Err(CompositorError::Encoder("av_hwframe_get_buffer failed".into()));
            }
            if av_hwframe_transfer_data(hw, sw, 0) < 0 {
                free_frame(sw);
                free_frame(hw);
                return Err(CompositorError::Encoder("av_hwframe_transfer_data failed".into()));
            }
            free_frame(sw);
            self.send_and_drain(hw)
        }
    }

    /// Zero-copy feed: wrap the dmabuf as DRM-PRIME → VPP map+CSC → encode.
    fn encode_dma(&mut self, dmabuf: &Dmabuf) -> crate::Result<Option<Vec<u8>>> {
        // Build the input frame, then push it through the graph to get an NV12 VAAPI frame.
        let in_frame = {
            let Feed::Dma { drm_frames, .. } = &self.feed else {
                return Err(CompositorError::Encoder("encode_dma on a non-DMA feed".into()));
            };
            dmabuf_import::drm_prime_frame(dmabuf, drm_frames)?
        };
        let out = {
            let Feed::Dma { graph, .. } = &mut self.feed else {
                return Err(CompositorError::Encoder("encode_dma feed changed".into()));
            };
            graph.process(in_frame)?
        };
        match out {
            Some(hw) => unsafe { self.send_and_drain(hw) },
            None => Ok(None),
        }
    }

    /// Stamp pts / forced-IDR on a VAAPI NV12 `hw` frame, submit it, and drain Annex-B
    /// packets. Frees `hw`.
    unsafe fn send_and_drain(&mut self, hw: *mut AVFrame) -> crate::Result<Option<Vec<u8>>> {
        unsafe {
            (*hw).pts = self.pts;
            self.pts += 1;
            if self.force_idr {
                self.force_idr = false;
                (*hw).pict_type = AVPictureType::I;
            }
            let send = avcodec_send_frame(self.ctx, hw);
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
    fn submit(&mut self, frame: Frame<'_>) -> crate::Result<Option<Vec<u8>>> {
        match frame {
            Frame::Rgba(rgba) if matches!(self.feed, Feed::Cpu { .. }) => self.encode_cpu(&rgba),
            Frame::Dma(dmabuf) if matches!(self.feed, Feed::Dma { .. }) => self.encode_dma(dmabuf),
            _ => Err(CompositorError::Encoder(
                "frame kind does not match this VAAPI encoder's feed".into(),
            )),
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

/// Allocate, configure (low-latency CBR), and open an `h264_vaapi` encoder bound to the
/// NV12 VAAPI frames context `hw_frames` (the codec takes its own ref). Frees the context on
/// failure. `unsafe`: raw ffmpeg FFI.
unsafe fn open_h264_vaapi(
    w: i32,
    h: i32,
    fps: u32,
    bitrate_kbps: u32,
    keyframe_interval: u32,
    hw_frames: *mut AVBufferRef,
) -> crate::Result<*mut AVCodecContext> {
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
        (*ctx).rc_max_rate = bitrate; // CBR
        (*ctx).rc_min_rate = bitrate;
        (*ctx).rc_buffer_size = bitrate as i32; // ~1 s VBV
        (*ctx).hw_frames_ctx = av_buffer_ref(hw_frames);

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
        Ok(ctx)
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
