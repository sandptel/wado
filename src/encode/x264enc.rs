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

    /// Encode one ABGR8888 frame and return the H.264 Annex-B NAL bytes.
    pub fn encode_abgr(&mut self, abgr: &[u8]) -> Option<Vec<u8>> {
        let i420 = abgr_to_i420(abgr, self.width, self.height);
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

/// Convert ABGR8888 (byte order: A, B, G, R) to I420 planar YUV.
/// Uses BT.601 limited-range coefficients.
fn abgr_to_i420(abgr: &[u8], width: usize, height: usize) -> Vec<u8> {
    let y_size = width * height;
    let uv_size = (width / 2) * (height / 2);
    let mut out = vec![0u8; y_size + 2 * uv_size];

    for row in 0..height {
        for col in 0..width {
            let base = (row * width + col) * 4;
            let b = abgr[base + 1] as f32;
            let g = abgr[base + 2] as f32;
            let r = abgr[base + 3] as f32;
            out[row * width + col] = clamp_y(16.0 + 0.257 * r + 0.504 * g + 0.098 * b);
        }
    }

    let u_start = y_size;
    let v_start = y_size + uv_size;
    for row in 0..(height / 2) {
        for col in 0..(width / 2) {
            let mut r_sum = 0.0f32;
            let mut g_sum = 0.0f32;
            let mut b_sum = 0.0f32;
            for dr in 0..2usize {
                for dc in 0..2usize {
                    let base = ((row * 2 + dr) * width + (col * 2 + dc)) * 4;
                    b_sum += abgr[base + 1] as f32;
                    g_sum += abgr[base + 2] as f32;
                    r_sum += abgr[base + 3] as f32;
                }
            }
            let r = r_sum / 4.0;
            let g = g_sum / 4.0;
            let b = b_sum / 4.0;
            let uv_idx = row * (width / 2) + col;
            out[u_start + uv_idx] = clamp_uv(128.0 - 0.148 * r - 0.291 * g + 0.439 * b);
            out[v_start + uv_idx] = clamp_uv(128.0 + 0.439 * r - 0.368 * g - 0.071 * b);
        }
    }

    out
}

#[inline] fn clamp_y(v: f32) -> u8  { v.round().clamp(16.0, 235.0) as u8 }
#[inline] fn clamp_uv(v: f32) -> u8 { v.round().clamp(16.0, 240.0) as u8 }
