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

/// Read pixels from the currently-bound GlesRenderer framebuffer into a
/// plain `Vec<u8>` in ABGR8888 order, with the GL y-flip corrected.
pub fn capture_frame(
    renderer: &mut GlesRenderer,
    framebuffer: &GlesTarget<'_>,
    size: Size<i32, Buffer>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let region = Rectangle::new((0, 0).into(), size);

    let mapping = renderer.copy_framebuffer(framebuffer, region, Fourcc::Abgr8888)?;

    let flipped = mapping.flipped();
    let raw = renderer.map_texture(&mapping)?;
    let bytes = raw.to_vec();

    if flipped {
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
