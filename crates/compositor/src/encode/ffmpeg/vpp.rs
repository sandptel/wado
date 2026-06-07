//! VAAPI VPP filtergraph for the zero-copy path: maps an imported DRM-PRIME dmabuf onto a
//! VAAPI surface and converts RGBA → NV12 **on the GPU** (`hwmap` + `scale_vaapi`), so the
//! encoder receives NV12 VAAPI surfaces with no CPU copy.
//!
//! Graph: `buffer(DRM_PRIME) → hwmap=derive_device=vaapi → scale_vaapi=format=nv12 →
//! buffersink`. The encoder binds to the buffersink's NV12 VAAPI frames context.

use std::ptr;

use ffmpeg_the_third::ffi::{
    AVBufferRef, AVFilterContext, AVFilterGraph, AVPixelFormat, AVRational, av_buffer_ref,
    av_buffer_unref, av_buffersink_get_frame, av_buffersink_get_hw_frames_ctx,
    av_buffersrc_add_frame_flags, av_buffersrc_parameters_alloc, av_buffersrc_parameters_set,
    av_frame_alloc, av_frame_free, av_free, av_strdup, avfilter_get_by_name,
    avfilter_graph_alloc, avfilter_graph_alloc_filter, avfilter_graph_config,
    avfilter_graph_create_filter, avfilter_graph_free, avfilter_graph_parse_ptr,
    avfilter_init_str, avfilter_inout_alloc, avfilter_inout_free,
};

use super::hwcontext::FramesRef;
use super::{AVERROR_EAGAIN, AVERROR_EOF};
use crate::CompositorError;

/// A configured VAAPI VPP filtergraph. Owns the graph (which owns its filter contexts) and
/// frees it on drop.
pub struct FilterGraph {
    graph: *mut AVFilterGraph,
    src: *mut AVFilterContext,
    sink: *mut AVFilterContext,
}

impl Drop for FilterGraph {
    fn drop(&mut self) {
        unsafe { avfilter_graph_free(&mut self.graph) };
    }
}

impl FilterGraph {
    /// Build the graph. `drm_frames` is the DRM-PRIME input frames pool (the source of the
    /// dmabuf frames); `width`/`height`/`fps` describe the stream.
    pub fn new(drm_frames: &FramesRef, width: i32, height: i32, fps: u32) -> crate::Result<Self> {
        unsafe {
            let graph = avfilter_graph_alloc();
            if graph.is_null() {
                return Err(CompositorError::Encoder("avfilter_graph_alloc failed".into()));
            }
            // Hold in `this` so any early `return Err` frees the graph via Drop.
            let mut this = FilterGraph { graph, src: ptr::null_mut(), sink: ptr::null_mut() };

            let buffersrc = avfilter_get_by_name(c"buffer".as_ptr());
            let buffersink = avfilter_get_by_name(c"buffersink".as_ptr());
            if buffersrc.is_null() || buffersink.is_null() {
                return Err(CompositorError::Encoder("buffer/buffersink filter missing".into()));
            }

            // ── buffersrc (DRM-PRIME input) ───────────────────────────────────────
            // A HW input format (DRM_PRIME) must have its hw_frames_ctx set *before* the
            // filter is initialised, so alloc it unconfigured, set params, then init.
            this.src = avfilter_graph_alloc_filter(graph, buffersrc, c"in".as_ptr());
            if this.src.is_null() {
                return Err(CompositorError::Encoder("alloc buffersrc failed".into()));
            }
            let par = av_buffersrc_parameters_alloc();
            if par.is_null() {
                return Err(CompositorError::Encoder("buffersrc_parameters_alloc failed".into()));
            }
            (*par).format = AVPixelFormat::DRM_PRIME.0;
            (*par).width = width;
            (*par).height = height;
            (*par).time_base = AVRational { num: 1, den: fps.max(1) as i32 };
            (*par).hw_frames_ctx = av_buffer_ref(drm_frames.0);
            let ret = av_buffersrc_parameters_set(this.src, par);
            av_buffer_unref(&mut (*par).hw_frames_ctx);
            av_free(par as *mut _);
            if ret < 0 {
                return Err(CompositorError::Encoder(format!("buffersrc_parameters_set: {ret}")));
            }
            let ret = avfilter_init_str(this.src, ptr::null());
            if ret < 0 {
                return Err(CompositorError::Encoder(format!("buffersrc init failed: {ret}")));
            }

            // ── buffersink ────────────────────────────────────────────────────────
            let ret = avfilter_graph_create_filter(
                &mut this.sink,
                buffersink,
                c"out".as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                return Err(CompositorError::Encoder(format!("create buffersink failed: {ret}")));
            }

            // ── parse the middle chain and link in→[hwmap,scale_vaapi]→out ────────
            let outputs = avfilter_inout_alloc();
            let inputs = avfilter_inout_alloc();
            if outputs.is_null() || inputs.is_null() {
                if !outputs.is_null() {
                    let mut o = outputs;
                    avfilter_inout_free(&mut o);
                }
                if !inputs.is_null() {
                    let mut i = inputs;
                    avfilter_inout_free(&mut i);
                }
                return Err(CompositorError::Encoder("inout_alloc failed".into()));
            }
            (*outputs).name = av_strdup(c"in".as_ptr());
            (*outputs).filter_ctx = this.src;
            (*outputs).pad_idx = 0;
            (*outputs).next = ptr::null_mut();
            (*inputs).name = av_strdup(c"out".as_ptr());
            (*inputs).filter_ctx = this.sink;
            (*inputs).pad_idx = 0;
            (*inputs).next = ptr::null_mut();

            let mut outputs_p = outputs;
            let mut inputs_p = inputs;
            let ret = avfilter_graph_parse_ptr(
                graph,
                c"hwmap=derive_device=vaapi,scale_vaapi=format=nv12".as_ptr(),
                &mut inputs_p,
                &mut outputs_p,
                ptr::null_mut(),
            );
            avfilter_inout_free(&mut inputs_p);
            avfilter_inout_free(&mut outputs_p);
            if ret < 0 {
                return Err(CompositorError::Encoder(format!("graph_parse_ptr failed: {ret}")));
            }

            let ret = avfilter_graph_config(graph, ptr::null_mut());
            if ret < 0 {
                return Err(CompositorError::Encoder(format!("graph_config failed: {ret}")));
            }

            Ok(this)
        }
    }

    /// The NV12 VAAPI frames context the graph outputs — bind the encoder to this. Returns a
    /// new ref the caller owns.
    pub fn output_frames_ctx(&self) -> crate::Result<*mut AVBufferRef> {
        let f = unsafe { av_buffersink_get_hw_frames_ctx(self.sink) };
        if f.is_null() {
            return Err(CompositorError::Encoder("buffersink has no hw_frames_ctx".into()));
        }
        Ok(unsafe { av_buffer_ref(f) })
    }

    /// Push one DRM-PRIME `in_frame` through the graph and pull the NV12 VAAPI result into a
    /// freshly allocated frame. Returns the output frame (caller frees it), or `None` if the
    /// graph buffered without output. Consumes `in_frame` (freed here).
    pub fn process(
        &mut self,
        in_frame: *mut ffmpeg_the_third::ffi::AVFrame,
    ) -> crate::Result<Option<*mut ffmpeg_the_third::ffi::AVFrame>> {
        unsafe {
            let ret = av_buffersrc_add_frame_flags(self.src, in_frame, 0);
            // The src moves the frame's contents out; free the (now empty) shell either way.
            let mut f = in_frame;
            av_frame_free(&mut f);
            if ret < 0 {
                return Err(CompositorError::Encoder(format!("buffersrc_add_frame: {ret}")));
            }

            let out = av_frame_alloc();
            if out.is_null() {
                return Err(CompositorError::Encoder("av_frame_alloc(vpp out) failed".into()));
            }
            let ret = av_buffersink_get_frame(self.sink, out);
            if ret == 0 {
                Ok(Some(out))
            } else {
                let mut o = out;
                av_frame_free(&mut o);
                if ret == AVERROR_EAGAIN || ret == AVERROR_EOF {
                    Ok(None)
                } else {
                    Err(CompositorError::Encoder(format!("buffersink_get_frame: {ret}")))
                }
            }
        }
    }
}
