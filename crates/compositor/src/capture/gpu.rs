//! GPU bring-up: open the DRM render node as a **GBM device** and initialise the EGL
//! display **from it**, so the `GlesRenderer` can both render to an offscreen renderbuffer
//! (CPU `ExportMem` path) *and* allocate + import DMA-BUFs (zero-copy path). Initialising
//! EGL from the GBM device guarantees the renderer and the dmabuf allocator sit on the
//! same GPU — avoiding cross-device import failures.
//!
//! Falls back to a **surfaceless** EGL display (no GBM) when the render node can't be
//! opened; that path supports only the CPU capture tiers (no DMA-BUF allocation).

use std::fs::File;
use std::os::fd::OwnedFd;

use smithay::backend::allocator::gbm::GbmDevice;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::egl::{EGLDisplay, native::EGLSurfacelessDisplay};
use smithay::utils::DeviceFd;

use crate::CompositorError;

/// A clone-able GBM device (the fd is `Arc`-shared via [`DrmDeviceFd`]) so both the EGL
/// display and the dmabuf allocator can hold one.
pub type Gbm = GbmDevice<DrmDeviceFd>;

/// The GPU resources a session renders on. `gbm` is `Some` only when a render node opened —
/// it gates the DMA-BUF (zero-copy) capture tier.
pub struct Gpu {
    pub egl: EGLDisplay,
    pub gbm: Option<Gbm>,
}

/// Open the GPU: prefer a GBM-backed (dmabuf-capable) EGL display, else surfaceless.
pub fn open() -> crate::Result<Gpu> {
    if let Some((gbm, egl)) = open_gbm() {
        return Ok(Gpu { egl, gbm: Some(gbm) });
    }
    let egl = unsafe { EGLDisplay::new(EGLSurfacelessDisplay) }
        .map_err(|e| CompositorError::Renderer(format!("EGLDisplay::new(surfaceless): {e}")))?;
    tracing::warn!("no GBM render node — EGL surfaceless (CPU capture tiers only)");
    Ok(Gpu { egl, gbm: None })
}

/// Try to open the first DRM render node as a GBM device and build an EGL display from it.
/// Returns `None` (so the caller falls back to surfaceless) on any failure.
fn open_gbm() -> Option<(Gbm, EGLDisplay)> {
    let node = first_render_node()?;
    let file = File::options().read(true).write(true).open(&node).ok()?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(OwnedFd::from(file)));
    let gbm = GbmDevice::new(drm_fd).ok()?;
    // SAFETY: `gbm` (the fd is Arc-shared) outlives the returned `EGLDisplay` — the caller
    // stores a clone on the session and the renderer's context keeps its own ref.
    let egl = unsafe { EGLDisplay::new(gbm.clone()) }.ok()?;
    tracing::info!(node, "GPU: GBM device + EGL display (DMA-BUF capable)");
    Some((gbm, egl))
}

/// First DRM render node (`/dev/dri/renderD*`). Kept here so `capture` doesn't depend on
/// the encode side (the ffmpeg backend has its own copy for the VAAPI device).
fn first_render_node() -> Option<String> {
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
