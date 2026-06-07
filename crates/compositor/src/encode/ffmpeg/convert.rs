//! CPU colour conversion for the VAAPI path: `Fourcc::Abgr8888` (`[R,G,B,A]` per pixel,
//! what [`crate::capture::mem::capture_frame`] returns) → planar **NV12** (Y plane, then
//! interleaved Cb/Cr at half resolution).
//!
//! NV12 is what we upload into the VAAPI surface (`sw_format = NV12`). Coefficients are
//! **BT.601 studio swing (limited/TV range)**, identical to the integer math in
//! [`crate::encode::x264enc`]'s `rgba_to_i420`, so the hardware and software encoders
//! produce matching colour.
//!
//! Step 1 keeps this CPU conversion (we still capture via `ExportMem`). Step 2 (DMA-BUF
//! zero-copy) replaces it with a GPU import and drops this module from the hot path.

/// Convert an `Abgr8888` (memory `[R,G,B,A]`) buffer to a tightly-packed NV12 buffer:
/// `width*height` luma bytes followed by `width*(height/2)` interleaved Cb/Cr bytes.
/// `width` and `height` must be even.
pub fn rgba_to_nv12(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = width * (height / 2); // (w/2 * h/2) pairs * 2 bytes = w * h/2
    let mut out = vec![0u8; y_size + uv_size];

    // Luma — one Y per pixel. Y = ((66*R + 129*G + 25*B + 128) >> 8) + 16
    for row in 0..height {
        for col in 0..width {
            let base = (row * width + col) * 4;
            let r = rgba[base] as i32;
            let g = rgba[base + 1] as i32;
            let b = rgba[base + 2] as i32;
            out[row * width + col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8;
        }
    }

    // Chroma — one interleaved Cb/Cr pair per 2×2 block (4:2:0). Row stride = width.
    // U = ((-38*R - 74*G + 112*B + 128) >> 8) + 128
    // V = (( 112*R - 94*G - 18*B + 128) >> 8) + 128
    let uv = y_size;
    for row in 0..(height / 2) {
        for col in 0..(width / 2) {
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for dr in 0..2usize {
                for dc in 0..2usize {
                    let base = ((row * 2 + dr) * width + (col * 2 + dc)) * 4;
                    r_sum += rgba[base] as i32;
                    g_sum += rgba[base + 1] as i32;
                    b_sum += rgba[base + 2] as i32;
                }
            }
            let r = r_sum >> 2;
            let g = g_sum >> 2;
            let b = b_sum >> 2;
            let idx = uv + row * width + col * 2;
            out[idx] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8;
            out[idx + 1] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8;
        }
    }

    out
}
