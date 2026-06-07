//! Frame capture: turning the rendered GLES framebuffer into something the encoder can
//! consume. Two strategies behind one trait so the render tick is capture-agnostic:
//! [`mem::MemTarget`] (GPU→CPU read-back, `ExportMem` — the robust fallback, invariant #3)
//! and `dmabuf::DmaTarget` (zero-copy DMA-BUF kept on the GPU — added in M3 Step 2).
//!
//! The encoder receives a [`Frame`]: either CPU `Rgba` bytes or a borrowed GPU `Dmabuf`.
//! The `Dmabuf` is the **backend-neutral** hand-off type — a future GStreamer encoder
//! consumes the same thing the ffmpeg backend does.

use std::borrow::Cow;

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget};

pub mod dmabuf;
pub mod gpu;
pub mod mem;

pub use dmabuf::DmaTarget;
pub use mem::MemTarget;

/// One captured frame, handed to the encoder for this tick.
pub enum Frame<'a> {
    /// CPU pixels in `Fourcc::Abgr8888` memory layout (`[R, G, B, A]` per pixel). The
    /// `ExportMem` read-back path; consumed by x264 and the VAAPI CPU-upload feed.
    Rgba(Cow<'a, [u8]>),
    /// A GPU dma-buffer kept on the GPU (zero-copy). The encoder imports it directly
    /// (ffmpeg: DRM-PRIME → VAAPI). Borrowed from the capture target for this tick only.
    Dma(&'a Dmabuf),
}

/// A render-and-capture target: owns the framebuffer the compositor renders into, and
/// turns the rendered result into a [`Frame`]. One impl per capture strategy
/// ([`MemTarget`], and `DmaTarget` in Step 2).
pub trait CaptureTarget {
    /// Render one frame into this target by invoking `render` on the bound framebuffer,
    /// then return the captured [`Frame`] (borrowing `self` for the GPU `Dma` case).
    fn capture(
        &mut self,
        renderer: &mut GlesRenderer,
        render: &mut dyn FnMut(&mut GlesRenderer, &mut GlesTarget<'_>) -> crate::Result<()>,
    ) -> crate::Result<Frame<'_>>;

    /// Best-effort CPU read-back of the last rendered frame, in `Abgr8888` (for debug
    /// snapshots). `Ok(None)` when the target can't cheaply produce CPU pixels.
    fn current_rgba(&mut self, _renderer: &mut GlesRenderer) -> crate::Result<Option<Vec<u8>>> {
        Ok(None)
    }
}
