//! Build a DRM-PRIME `AVFrame` that references a Smithay [`Dmabuf`] zero-copy. The frame
//! carries an [`AVDRMFrameDescriptor`] (objects = the dmabuf fds + modifier, layers/planes =
//! the format + strides/offsets); pushed through the VAAPI VPP filtergraph
//! ([`super::vpp`]) it maps straight onto a VAAPI surface with no CPU round-trip.
//!
//! The dmabuf fds are **borrowed** — VAAPI dups them on import, so the caller only has to
//! keep the `Dmabuf` alive until the filter has consumed the frame.

use std::os::fd::AsRawFd;
use std::ptr;

use ffmpeg_the_third::ffi::{
    AVDRMFrameDescriptor, AVDRMLayerDescriptor, AVDRMObjectDescriptor, AVDRMPlaneDescriptor,
    AVFrame, AVPixelFormat, av_buffer_create, av_buffer_ref, av_frame_alloc, av_frame_free,
};
use smithay::backend::allocator::Buffer as _;
use smithay::backend::allocator::dmabuf::Dmabuf;

use super::hwcontext::FramesRef;
use crate::CompositorError;

/// `av_buffer` free callback: reclaim and drop the boxed descriptor.
unsafe extern "C" fn free_descriptor(_opaque: *mut std::ffi::c_void, data: *mut u8) {
    drop(unsafe { Box::from_raw(data as *mut AVDRMFrameDescriptor) });
}

/// Build a DRM-PRIME `AVFrame` for `dmabuf`, with `hw_frames_ctx` set to the DRM frames
/// pool `drm_frames`. Free the returned frame with `av_frame_free` (which drops the
/// descriptor). The dmabuf must outlive the frame's use in the filtergraph.
pub fn drm_prime_frame(dmabuf: &Dmabuf, drm_frames: &FramesRef) -> crate::Result<*mut AVFrame> {
    let format = dmabuf.format();
    let fourcc = format.code as u32;
    let modifier: u64 = format.modifier.into();
    let num_planes = dmabuf.num_planes();
    if num_planes == 0 || num_planes > 4 {
        return Err(CompositorError::Encoder(format!(
            "unsupported dmabuf plane count {num_planes}"
        )));
    }
    // Multi-fd dmabufs (tiled/multi-planar formats that span several GEM objects) cannot be
    // represented in a single AVDRMObjectDescriptor. Reject early so the caller's downgrade
    // path handles it gracefully rather than passing a malformed descriptor to native VAAPI —
    // a malformed descriptor would cause a driver-level fault that `catch_unwind` cannot catch.
    let fd_count = dmabuf.handles().count();
    if fd_count > 1 {
        return Err(CompositorError::Encoder(format!(
            "multi-fd dmabuf ({fd_count} objects) unsupported by DRM-PRIME import — \
             only single-object (Linear) dmabufs are supported"
        )));
    }
    let fd = dmabuf
        .handles()
        .next()
        .ok_or_else(|| CompositorError::Encoder("dmabuf has no fd".into()))?
        .as_raw_fd();
    let offsets: Vec<u32> = dmabuf.offsets().collect();
    let strides: Vec<u32> = dmabuf.strides().collect();

    // One object (the single dmabuf), one layer, `num_planes` planes (RGBA = 1).
    let mut desc: Box<AVDRMFrameDescriptor> = Box::new(unsafe { std::mem::zeroed() });
    desc.nb_objects = 1;
    desc.objects[0] = AVDRMObjectDescriptor { fd, size: 0, format_modifier: modifier };
    desc.nb_layers = 1;
    let mut layer: AVDRMLayerDescriptor = unsafe { std::mem::zeroed() };
    layer.format = fourcc;
    layer.nb_planes = num_planes as i32;
    for i in 0..num_planes {
        layer.planes[i] = AVDRMPlaneDescriptor {
            object_index: 0,
            offset: offsets.get(i).copied().unwrap_or(0) as isize,
            pitch: strides.get(i).copied().unwrap_or(0) as isize,
        };
    }
    desc.layers[0] = layer;

    let frame = unsafe { av_frame_alloc() };
    if frame.is_null() {
        return Err(CompositorError::Encoder("av_frame_alloc(drm_prime) failed".into()));
    }

    let desc_ptr = Box::into_raw(desc);
    let buf = unsafe {
        av_buffer_create(
            desc_ptr as *mut u8,
            std::mem::size_of::<AVDRMFrameDescriptor>(),
            Some(free_descriptor),
            ptr::null_mut(),
            0,
        )
    };
    if buf.is_null() {
        // Reclaim the descriptor and the frame so nothing leaks.
        drop(unsafe { Box::from_raw(desc_ptr) });
        unsafe {
            let mut f = frame;
            av_frame_free(&mut f);
        }
        return Err(CompositorError::Encoder("av_buffer_create(descriptor) failed".into()));
    }

    unsafe {
        (*frame).format = AVPixelFormat::DRM_PRIME.0;
        (*frame).width = dmabuf.width() as i32;
        (*frame).height = dmabuf.height() as i32;
        (*frame).data[0] = desc_ptr as *mut u8;
        (*frame).buf[0] = buf;
        (*frame).hw_frames_ctx = av_buffer_ref(drm_frames.0);
    }
    Ok(frame)
}
