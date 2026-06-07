//! VAAPI hardware-context plumbing: the `AVHWDeviceContext` (a VAAPI device on a DRM
//! render node) and the `AVHWFramesContext` (a pool of NV12 VAAPI surfaces the encoder
//! draws from). Both are reference-counted `AVBufferRef`s; the RAII wrappers here unref
//! them on drop. This is the only place that opens the GPU device.

use std::ffi::CString;
use std::ptr;

use ffmpeg_the_third::ffi::{
    AVBufferRef, AVHWDeviceType, AVHWFramesContext, AVPixelFormat, av_buffer_unref,
    av_hwdevice_ctx_create, av_hwdevice_ctx_create_derived, av_hwframe_ctx_alloc,
    av_hwframe_ctx_init,
};

use crate::CompositorError;

/// A VAAPI `AVHWDeviceContext` ref. Unrefs on drop.
pub struct DeviceRef(pub *mut AVBufferRef);

impl Drop for DeviceRef {
    fn drop(&mut self) {
        unsafe { av_buffer_unref(&mut self.0) };
    }
}

/// An `AVHWFramesContext` ref (a pool of NV12 VAAPI surfaces). Unrefs on drop.
pub struct FramesRef(pub *mut AVBufferRef);

impl Drop for FramesRef {
    fn drop(&mut self) {
        unsafe { av_buffer_unref(&mut self.0) };
    }
}

/// Pick the first DRM render node (`/dev/dri/renderD*`). Returns `None` if none exist.
pub fn first_render_node() -> Option<String> {
    let mut nodes: Vec<String> = std::fs::read_dir("/dev/dri")
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("renderD"))
        .map(|n| format!("/dev/dri/{n}"))
        .collect();
    nodes.sort();
    nodes.into_iter().next()
}

/// Open a VAAPI device on the given DRM render node (e.g. `/dev/dri/renderD128`).
pub fn create_vaapi_device(node: &str) -> crate::Result<DeviceRef> {
    let cnode =
        CString::new(node).map_err(|e| CompositorError::Encoder(format!("bad node path: {e}")))?;
    let mut dev: *mut AVBufferRef = ptr::null_mut();
    let ret = unsafe {
        av_hwdevice_ctx_create(
            &mut dev,
            AVHWDeviceType::VAAPI,
            cnode.as_ptr(),
            ptr::null_mut(),
            0,
        )
    };
    if ret < 0 || dev.is_null() {
        return Err(CompositorError::Encoder(format!(
            "av_hwdevice_ctx_create(VAAPI, {node}) failed: {ret}"
        )));
    }
    Ok(DeviceRef(dev))
}

/// Open a **DRM** hardware device on the given render node — the entry point for the
/// zero-copy DMA-BUF import path (a VAAPI device is then *derived* from it so the imported
/// dmabuf and the encoder share one GPU context).
pub fn create_drm_device(node: &str) -> crate::Result<DeviceRef> {
    let cnode =
        CString::new(node).map_err(|e| CompositorError::Encoder(format!("bad node path: {e}")))?;
    let mut dev: *mut AVBufferRef = ptr::null_mut();
    let ret = unsafe {
        av_hwdevice_ctx_create(&mut dev, AVHWDeviceType::DRM, cnode.as_ptr(), ptr::null_mut(), 0)
    };
    if ret < 0 || dev.is_null() {
        return Err(CompositorError::Encoder(format!(
            "av_hwdevice_ctx_create(DRM, {node}) failed: {ret}"
        )));
    }
    Ok(DeviceRef(dev))
}

/// Derive a VAAPI device from an existing DRM device (same GPU), so dmabufs imported via
/// DRM-PRIME map directly into VAAPI surfaces with no copy.
pub fn derive_vaapi_device(drm: &DeviceRef) -> crate::Result<DeviceRef> {
    let mut dev: *mut AVBufferRef = ptr::null_mut();
    let ret = unsafe {
        av_hwdevice_ctx_create_derived(&mut dev, AVHWDeviceType::VAAPI, drm.0, 0)
    };
    if ret < 0 || dev.is_null() {
        return Err(CompositorError::Encoder(format!(
            "av_hwdevice_ctx_create_derived(VAAPI from DRM) failed: {ret}"
        )));
    }
    Ok(DeviceRef(dev))
}

/// Allocate and initialise a **DRM-PRIME** frames pool on `device` with the given software
/// pixel format — the input frames context for the VPP filtergraph (each frame wraps an
/// imported dmabuf descriptor).
pub fn create_drm_prime_frames(
    device: &DeviceRef,
    sw_format: AVPixelFormat,
    width: i32,
    height: i32,
) -> crate::Result<FramesRef> {
    let frames = unsafe { av_hwframe_ctx_alloc(device.0) };
    if frames.is_null() {
        return Err(CompositorError::Encoder("av_hwframe_ctx_alloc(DRM) failed".into()));
    }
    unsafe {
        let ctx = (*frames).data as *mut AVHWFramesContext;
        (*ctx).format = AVPixelFormat::DRM_PRIME;
        (*ctx).sw_format = sw_format;
        (*ctx).width = width;
        (*ctx).height = height;
        let ret = av_hwframe_ctx_init(frames);
        if ret < 0 {
            let mut f = frames;
            av_buffer_unref(&mut f);
            return Err(CompositorError::Encoder(format!(
                "av_hwframe_ctx_init(DRM) failed: {ret}"
            )));
        }
    }
    Ok(FramesRef(frames))
}

/// Allocate and initialise an NV12 VAAPI frames pool bound to `device`.
pub fn create_nv12_frames(
    device: &DeviceRef,
    width: i32,
    height: i32,
    pool_size: i32,
) -> crate::Result<FramesRef> {
    let frames = unsafe { av_hwframe_ctx_alloc(device.0) };
    if frames.is_null() {
        return Err(CompositorError::Encoder("av_hwframe_ctx_alloc failed".into()));
    }
    // `AVBufferRef::data` points at the `AVHWFramesContext` we configure before init.
    unsafe {
        let ctx = (*frames).data as *mut AVHWFramesContext;
        (*ctx).format = AVPixelFormat::VAAPI;
        (*ctx).sw_format = AVPixelFormat::NV12;
        (*ctx).width = width;
        (*ctx).height = height;
        (*ctx).initial_pool_size = pool_size;
        let ret = av_hwframe_ctx_init(frames);
        if ret < 0 {
            let mut f = frames;
            av_buffer_unref(&mut f);
            return Err(CompositorError::Encoder(format!(
                "av_hwframe_ctx_init failed: {ret}"
            )));
        }
    }
    Ok(FramesRef(frames))
}
