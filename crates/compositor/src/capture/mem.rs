//! Host-memory capture (`ExportMem`): render into a `GlesRenderbuffer`, then read the
//! pixels back to the CPU as `Abgr8888`. The robust, always-available fallback
//! (invariant #3) — feeds x264 and the VAAPI CPU-upload path. No DMA-BUF, no modifiers.

use std::borrow::Cow;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, ExportMem, Offscreen, TextureMapping,
            gles::{GlesRenderbuffer, GlesRenderer, GlesTarget},
        },
    },
    utils::{Buffer, Rectangle, Size},
};

use crate::CompositorError;
use crate::capture::{CaptureTarget, Frame};

/// A CPU read-back capture target: owns the offscreen `GlesRenderbuffer` the compositor
/// renders into, and copies it to host memory each frame.
pub struct MemTarget {
    renderbuffer: GlesRenderbuffer,
    size: Size<i32, Buffer>,
}

impl MemTarget {
    /// Allocate the offscreen `Abgr8888` renderbuffer at `size`.
    pub fn new(renderer: &mut GlesRenderer, size: Size<i32, Buffer>) -> crate::Result<Self> {
        let renderbuffer = renderer
            .create_buffer(Fourcc::Abgr8888, size)
            .map_err(|e| CompositorError::Renderer(format!("create_buffer: {e}")))?;
        Ok(Self { renderbuffer, size })
    }
}

impl CaptureTarget for MemTarget {
    fn capture(
        &mut self,
        renderer: &mut GlesRenderer,
        render: &mut dyn FnMut(&mut GlesRenderer, &mut GlesTarget<'_>) -> crate::Result<()>,
    ) -> crate::Result<Frame<'_>> {
        let mut fb = renderer
            .bind(&mut self.renderbuffer)
            .map_err(|e| CompositorError::Renderer(format!("bind: {e}")))?;
        render(renderer, &mut fb)?;
        let pixels = capture_frame(renderer, &fb, self.size)?;
        drop(fb);
        Ok(Frame::Rgba(Cow::Owned(pixels)))
    }

    fn current_rgba(&mut self, renderer: &mut GlesRenderer) -> crate::Result<Option<Vec<u8>>> {
        let fb = renderer
            .bind(&mut self.renderbuffer)
            .map_err(|e| CompositorError::Renderer(format!("bind: {e}")))?;
        let pixels = capture_frame(renderer, &fb, self.size)?;
        Ok(Some(pixels))
    }
}

/// Read pixels from a bound `GlesRenderer` framebuffer into a `Vec<u8>`.
///
/// Returns `Fourcc::Abgr8888` wire format — on little-endian that is `[R, G, B, A]` per
/// pixel in memory (GL_RGBA + GL_UNSIGNED_BYTE order). The GL y-flip is corrected so row 0
/// is the top of the image.
pub fn capture_frame(
    renderer: &mut GlesRenderer,
    framebuffer: &GlesTarget<'_>,
    size: Size<i32, Buffer>,
) -> crate::Result<Vec<u8>> {
    let region = Rectangle::new((0, 0).into(), size);

    let mapping = renderer
        .copy_framebuffer(framebuffer, region, Fourcc::Abgr8888)
        .map_err(|e| CompositorError::Capture(format!("copy_framebuffer: {e}")))?;

    // flipped()=true means the buffer origin is already at the upper-left (screen
    // convention, row 0 = display top) — Smithay's render() bakes a flip180 into the GL
    // projection so glReadPixels returns rows in screen order. We only un-flip when
    // flipped()==false (still in GL lower-left convention).
    let flipped = mapping.flipped();
    let raw = renderer
        .map_texture(&mapping)
        .map_err(|e| CompositorError::Capture(format!("map_texture: {e}")))?;
    let bytes = raw.to_vec();

    if !flipped {
        Ok(flip_rows(bytes, size.w as usize, size.h as usize))
    } else {
        Ok(bytes)
    }
}

/// Reverse the row order of a tightly-packed ABGR image in-place.
fn flip_rows(mut data: Vec<u8>, width: usize, height: usize) -> Vec<u8> {
    let stride = width * 4;
    for row in 0..height / 2 {
        let top = row * stride;
        let bot = (height - 1 - row) * stride;
        for col in 0..stride {
            data.swap(top + col, bot + col);
        }
    }
    data
}
