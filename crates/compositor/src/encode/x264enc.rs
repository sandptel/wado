use x264::{Colorspace, Encoder, Image, Plane, Preset, Setup, Tune};

use wado_protocol::KeyframeMode;

use crate::CompositorError;
use crate::capture::Frame;
use crate::encode::encoder::VideoEncoder;

pub struct X264Encoder {
    encoder: Encoder,
    width: usize,
    height: usize,
    fps: u32,
    bitrate_kbps: u32,
    keyframe_interval: u32,
    keyframe_mode: KeyframeMode,
    preset: Preset,
    pts: i64,
    /// SPS + PPS NALs, prepended to every IDR frame so the stream is self-contained.
    headers: Vec<u8>,
    /// Set by force_idr_next(); cleared after the encoder is rebuilt.
    force_idr: bool,
}

impl X264Encoder {
    /// Create an x264 encoder configured for zero-latency streaming.
    /// `width` and `height` must be even (required by I420 chroma subsampling).
    pub fn new(
        width: u32,
        height: u32,
        fps: u32,
        bitrate_kbps: u32,
        keyframe_interval: u32,
        keyframe_mode: KeyframeMode,
        preset: Preset,
    ) -> crate::Result<Self> {
        let w = width as usize;
        let h = height as usize;
        let mut encoder =
            build_encoder(w, h, fps, bitrate_kbps, keyframe_interval, keyframe_mode, preset)?;
        let headers = encoder
            .headers()
            .ok()
            .map(|d| d.entirety().to_vec())
            .unwrap_or_default();
        Ok(Self {
            encoder,
            width: w,
            height: h,
            fps,
            bitrate_kbps,
            keyframe_interval,
            keyframe_mode,
            preset,
            pts: 0,
            headers,
            force_idr: false,
        })
    }

    /// Encode one frame in the pixel format returned by GlesRenderer::copy_framebuffer
    /// (Fourcc::Abgr8888 — memory layout [R,G,B,A] on little-endian, i.e. GL_RGBA).
    pub fn encode_rgba(&mut self, rgba: &[u8]) -> Option<Vec<u8>> {
        if self.force_idr {
            self.force_idr = false;
            self.rebuild();
        }
        let i420 = rgba_to_i420(rgba, self.width, self.height);
        self.encode_i420(&i420)
    }

    /// Request that the next encoded frame be a forced IDR keyframe.
    ///
    /// Useful when a new client connects mid-stream and needs to sync immediately
    /// rather than waiting up to `keyframe_interval` frames for the next scheduled IDR.
    /// Implemented by rebuilding the encoder; the pts counter is not reset.
    pub fn force_idr_next(&mut self) {
        self.force_idr = true;
    }

    /// SPS + PPS header bytes — send these before the first frame of a new session.
    /// For ongoing streams, headers are automatically prepended to every IDR frame.
    pub fn headers(&mut self) -> Vec<u8> {
        self.headers.clone()
    }

    fn rebuild(&mut self) {
        match build_encoder(
            self.width,
            self.height,
            self.fps,
            self.bitrate_kbps,
            self.keyframe_interval,
            self.keyframe_mode,
            self.preset,
        ) {
            Ok(mut enc) => {
                let new_headers = enc
                    .headers()
                    .ok()
                    .map(|d| d.entirety().to_vec())
                    .unwrap_or_default();
                self.encoder = enc;
                self.headers = new_headers;
                // pts intentionally NOT reset — decoder cares about continuity of DTS, not
                // absolute values, and our zero_latency config has no B-frame reordering.
            }
            Err(e) => tracing::error!("x264 encoder rebuild failed: {e}"),
        }
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

        let (data, picture) = self.encoder.encode(pts, image).ok()?;
        let nal_bytes = data.entirety();
        if nal_bytes.is_empty() {
            return None;
        }

        // Prepend SPS+PPS before every IDR so a late-joining client can sync
        // without waiting for the next out-of-band header send.
        if picture.keyframe() {
            let mut out = Vec::with_capacity(self.headers.len() + nal_bytes.len());
            out.extend_from_slice(&self.headers);
            out.extend_from_slice(nal_bytes);
            Some(out)
        } else {
            Some(nal_bytes.to_vec())
        }
    }
}

impl VideoEncoder for X264Encoder {
    fn submit(&mut self, frame: Frame<'_>) -> crate::Result<Option<Vec<u8>>> {
        match frame {
            Frame::Rgba(rgba) => Ok(self.encode_rgba(&rgba)),
            Frame::Dma(_) => Err(CompositorError::Encoder(
                "x264 (software) cannot consume a DMA-BUF frame".into(),
            )),
        }
    }

    fn force_idr_next(&mut self) {
        X264Encoder::force_idr_next(self);
    }
}

fn build_encoder(
    width: usize,
    height: usize,
    fps: u32,
    bitrate_kbps: u32,
    keyframe_interval: u32,
    keyframe_mode: KeyframeMode,
    preset: Preset,
) -> crate::Result<Encoder> {
    // zero_latency=true (the 4th `Setup::preset` arg) → no B-frames, no lookahead delay, flush
    // every frame. The software path is the emergency fallback, so it *always* runs low-latency
    // regardless of the client's zero-latency toggle (the toggle is a hardware-facing knob).
    // scenecut_threshold(0) disables scene-cut detection so IDR timing is deterministic.
    //
    // Keyframe strategy. The `x264` 0.5 crate has no intra-refresh API, so we mirror the VAAPI
    // path: `OnDemand` uses a huge keyframe interval (no periodic IDR — IDRs come from the
    // force_idr rebuild on connect/PLI); `Periodic` pins a fixed min==max cadence.
    let (max_ki, min_ki) = match keyframe_mode {
        KeyframeMode::OnDemand => {
            (fps.max(1).saturating_mul(3600).min(i32::MAX as u32) as i32, 1)
        }
        KeyframeMode::Periodic => {
            let ki = keyframe_interval.max(1) as i32;
            (ki, ki)
        }
    };
    let encoder = Setup::preset(preset, Tune::None, false, true)
        .fps(fps, 1)
        .bitrate(bitrate_kbps as i32)
        .baseline()
        .max_keyframe_interval(max_ki)
        .min_keyframe_interval(min_ki)
        .scenecut_threshold(0)
        .annexb(true)
        .build(Colorspace::I420, width as i32, height as i32)
        .map_err(|e| crate::CompositorError::Encoder(format!("x264 build: {e:?}")))?;
    Ok(encoder)
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
