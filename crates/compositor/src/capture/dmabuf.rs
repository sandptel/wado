//! Zero-copy DMA-BUF capture: render the compositor into a GPU dma-buffer and hand it to
//! the encoder **without** a CPU round-trip. The backend-neutral half of the zero-copy
//! path — it produces a Smithay [`Dmabuf`], the hand-off type any encoder (ffmpeg now,
//! GStreamer later) imports.
//!
//! A small [`Swapchain`] (max 4 buffers) of `Abgr8888` dmabufs is rendered into round-robin
//! so the GPU isn't stalled waiting for the encoder to finish reading the previous frame.
//! Modifier choice is caller-supplied: `headless::build_tier` queries the GPU's preferred
//! render formats (`EGLContext::dmabuf_render_formats`) and runs a self-test before
//! committing; Linear is always the final fallback (invariant #3 / invariant #6).

use smithay::backend::{
    allocator::{
        Fourcc, Modifier, Slot, Swapchain,
        dmabuf::{AsDmabuf, Dmabuf},
        gbm::{GbmAllocator, GbmBuffer, GbmBufferFlags},
    },
    drm::DrmDeviceFd,
    renderer::{
        Bind,
        gles::{GlesRenderer, GlesTarget},
    },
};
use smithay::utils::{Buffer, Size};

use crate::CompositorError;
use crate::capture::{CaptureTarget, Frame, gpu::Gbm};

/// A zero-copy capture target: a swapchain of `Abgr8888` dma-buffers the compositor renders
/// into, handed to the encoder as a borrowed [`Dmabuf`].
pub struct DmaTarget {
    swapchain: Swapchain<GbmAllocator<DrmDeviceFd>>,
    /// The slot currently handed to the encoder; kept alive until the next `capture` so its
    /// buffer isn't recycled while the encoder still references it.
    current: Option<Slot<GbmBuffer>>,
}

impl DmaTarget {
    /// Build a swapchain on `gbm` at `size` using the provided `modifiers` list.
    /// GBM will allocate using the first modifier it supports; pass `[preferred…, Linear]`
    /// to prefer tiled layouts while keeping Linear as a fallback within the swapchain.
    /// The caller (headless::build_tier) is responsible for running a self-test before
    /// committing to any non-Linear modifier (invariant #6).
    pub fn new(gbm: Gbm, size: Size<i32, Buffer>, modifiers: Vec<Modifier>) -> crate::Result<Self> {
        let allocator = GbmAllocator::new(gbm, GbmBufferFlags::RENDERING);
        let swapchain = Swapchain::new(
            allocator,
            size.w as u32,
            size.h as u32,
            Fourcc::Abgr8888,
            modifiers,
        );
        Ok(Self { swapchain, current: None })
    }
}

impl CaptureTarget for DmaTarget {
    fn capture(
        &mut self,
        renderer: &mut GlesRenderer,
        render: &mut dyn FnMut(&mut GlesRenderer, &mut GlesTarget<'_>) -> crate::Result<()>,
    ) -> crate::Result<Frame<'_>> {
        let slot = self
            .swapchain
            .acquire()
            .map_err(|e| CompositorError::Capture(format!("swapchain acquire: {e}")))?
            .ok_or_else(|| CompositorError::Capture("no free dmabuf slot".into()))?;

        // `Slot::export` caches the Dmabuf in the slot's userdata, so the same underlying
        // buffer is returned every tick — the renderer's EGLImage cache then hits.
        let mut dmabuf = slot
            .export()
            .map_err(|e| CompositorError::Capture(format!("dmabuf export: {e}")))?;

        {
            let mut fb = renderer
                .bind(&mut dmabuf)
                .map_err(|e| CompositorError::Renderer(format!("bind dmabuf: {e}")))?;
            render(renderer, &mut fb)?;
        }

        self.current = Some(slot);
        let dref = self
            .current
            .as_ref()
            .unwrap()
            .userdata()
            .get::<Dmabuf>()
            .ok_or_else(|| CompositorError::Capture("dmabuf userdata missing".into()))?;
        Ok(Frame::Dma(dref))
    }
}
