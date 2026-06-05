use x264::{Colorspace, Encoder, Image, Plane, Preset, Setup, Tune};

pub struct X264Encoder {
    encoder: Encoder,
    width: usize,
    height: usize,
    pts: i64,
}

impl X264Encoder {
    /// Create an x264 encoder configured for zero-latency streaming.
    /// `width` and `height` must be even (required by I420 chroma subsampling).
    pub fn new(width: u32, height: u32, fps: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let w = width as usize;
        let h = height as usize;

        // zero_latency=true → no B-frames, no lookahead delay, flush every frame.
        let encoder = Setup::preset(Preset::Ultrafast, Tune::None, false, true)
            .fps(fps, 1)
            .bitrate(4000)
            .baseline()
            .max_keyframe_interval(30)
            .annexb(true)
            .build(Colorspace::I420, w as i32, h as i32)
            .map_err(|_| "x264 encoder build failed")?;

        Ok(Self { encoder, width: w, height: h, pts: 0 })
    }

    /// Encode one frame in the pixel format returned by GlesRenderer::copy_framebuffer
    /// (Fourcc::Abgr8888 — memory layout [R,G,B,A] on little-endian, i.e. GL_RGBA).
    pub fn encode_rgba(&mut self, rgba: &[u8]) -> Option<Vec<u8>> {
        let i420 = rgba_to_i420(rgba, self.width, self.height);
        self.encode_i420(&i420)
    }

    fn encode_i420(&mut self, i420: &[u8]) -> Option<Vec<u8>> {
        let w = self.width;
        let h = self.height;

        let y = &i420[..w * h];
        let u = &i420[w * h..w * h + (w / 2) * (h / 2)];
        let v = &i420[w * h + (w / 2) * (h / 2)..];

        let image = Image::new(
            Colorspace::I420,
            w as i32,
            h as i32,
            &[
                Plane { stride: w as i32, data: y },
                Plane { stride: (w / 2) as i32, data: u },
                Plane { stride: (w / 2) as i32, data: v },
            ],
        );

        let pts = self.pts;
        self.pts += 1;

        let (data, _picture) = self.encoder.encode(pts, image).ok()?;
        let bytes = data.entirety().to_vec();
        if bytes.is_empty() { None } else { Some(bytes) }
    }

    /// SPS + PPS header bytes — send these once before the first frame.
    pub fn headers(&mut self) -> Vec<u8> {
        self.encoder.headers().ok().map(|h| h.entirety().to_vec()).unwrap_or_default()
    }
}

/// Convert a pixel buffer in Fourcc::Abgr8888 format to I420 planar YUV.
///
/// Memory layout: [R=0, G=1, B=2, A=3] per pixel.  DRM "ABGR8888" names the
/// 32-bit integer from MSB→LSB, so on little-endian R is at byte offset 0 —
/// the same as GL_RGBA + GL_UNSIGNED_BYTE from glReadPixels.
///
/// Coefficients: BT.601 studio swing (limited/TV range), integer arithmetic.
/// These are identical to the coefficients used by libyuv (ABGRToI420) and
/// FFmpeg's swscale, ensuring correct colour on hardware decoders.
fn rgba_to_i420(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut out = vec![0u8; y_size + 2 * uv_size];

    // Luma — one Y sample per pixel.
    // Y = ((66*R + 129*G + 25*B + 128) >> 8) + 16
    for row in 0..height {
        for col in 0..width {
            let base = (row * width + col) * 4;
            let r = rgba[base]     as i32;
            let g = rgba[base + 1] as i32;
            let b = rgba[base + 2] as i32;
            out[row * width + col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8;
        }
    }

    // Chroma — one U/V sample per 2×2 block (4:2:0 subsampling).
    // U = ((-38*R - 74*G + 112*B + 128) >> 8) + 128
    // V = ((112*R - 94*G - 18*B + 128) >> 8) + 128
    let u_start = y_size;
    let v_start = y_size + uv_size;
    for row in 0..(height / 2) {
        for col in 0..(width / 2) {
            let mut r_sum = 0i32;
            let mut g_sum = 0i32;
            let mut b_sum = 0i32;
            for dr in 0..2usize {
                for dc in 0..2usize {
                    let base = ((row * 2 + dr) * width + (col * 2 + dc)) * 4;
                    r_sum += rgba[base]     as i32;
                    g_sum += rgba[base + 1] as i32;
                    b_sum += rgba[base + 2] as i32;
                }
            }
            // Divide by 4 via right-shift, then apply chroma coefficients.
            let r = r_sum >> 2;
            let g = g_sum >> 2;
            let b = b_sum >> 2;
            let idx = row * (width / 2) + col;
            out[u_start + idx] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8;
            out[v_start + idx] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8;
        }
    }

    out
}
