use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            ExportMem, TextureMapping,
            gles::{GlesRenderer, GlesTarget},
        },
    },
    utils::{Buffer, Rectangle, Size},
};

/// Read pixels from the currently-bound GlesRenderer framebuffer.
///
/// Returns a `Vec<u8>` in Fourcc::Abgr8888 wire format — which on little-endian
/// is [R, G, B, A] per pixel in memory (GL_RGBA + GL_UNSIGNED_BYTE order).
/// The GL y-flip is corrected so row 0 is the top of the image.
pub fn capture_frame(
    renderer: &mut GlesRenderer,
    framebuffer: &GlesTarget<'_>,
    size: Size<i32, Buffer>,
) -> crate::Result<Vec<u8>> {
    let region = Rectangle::new((0, 0).into(), size);

    let mapping = renderer
        .copy_framebuffer(framebuffer, region, Fourcc::Abgr8888)
        .map_err(|e| crate::CompositorError::Capture(format!("copy_framebuffer: {e}")))?;

    // flipped()=true means "y-axis is flipped compared to lower-left=(0,0)" — i.e. the
    // buffer origin IS already at the upper-left (screen convention, row 0 = display top).
    // Smithay's render() bakes a flip180 into the GL projection so that glReadPixels
    // returns rows in screen order.  We only need to un-flip when flipped()==false, which
    // would mean the data is still in GL lower-left convention.
    let flipped = mapping.flipped();
    let raw = renderer
        .map_texture(&mapping)
        .map_err(|e| crate::CompositorError::Capture(format!("map_texture: {e}")))?;
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
