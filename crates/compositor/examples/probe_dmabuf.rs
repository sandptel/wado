//! Standalone zero-copy (DMA-BUF) VAAPI check — no Wayland, no event loop.
//!
//! Opens the GPU (GBM + EGL), renders a solid colour into a DMA-BUF swapchain, imports each
//! dmabuf into VAAPI (DRM-PRIME → VPP RGBA→NV12) and encodes it with `h264_vaapi`, reporting
//! the Annex-B bytes. Use this to verify the **Tier A** zero-copy path in isolation before
//! the server commits a session to it.
//!
//! Usage:
//!   cargo run -p wado-compositor --example probe_dmabuf   # (sandbox disabled — opens the GPU)

use smithay::backend::{
    egl::EGLContext,
    renderer::{Color32F, Frame as _, Renderer as _, gles::GlesRenderer},
};
use smithay::utils::{Physical, Rectangle, Size, Transform};

use wado_compositor::CompositorError;
use wado_compositor::capture::{CaptureTarget, DmaTarget, gpu};
use wado_compositor::encode::encoder::VideoEncoder;
use wado_compositor::encode::ffmpeg::{FfmpegVaapiEncoder, hwcontext::first_render_node};

fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let Some(node) = first_render_node() else {
        println!("no render node — cannot test");
        return;
    };
    let gpu = match gpu::open() {
        Ok(g) => g,
        Err(e) => {
            println!("gpu::open failed: {e}");
            return;
        }
    };
    let Some(gbm) = gpu.gbm else {
        println!("no GBM device (surfaceless) — DMA-BUF path unavailable");
        return;
    };
    let egl_context = EGLContext::new(&gpu.egl).expect("EGLContext");
    let mut renderer = unsafe { GlesRenderer::new(egl_context).expect("GlesRenderer") };

    let (w, h) = (1280u32, 720u32);
    let size: Size<i32, smithay::utils::Buffer> = (w as i32, h as i32).into();
    let mut dma = match DmaTarget::new(gbm, size) {
        Ok(d) => d,
        Err(e) => {
            println!("DmaTarget::new failed: {e}");
            return;
        }
    };
    let mut enc = match FfmpegVaapiEncoder::new_dma(&node, w, h, 60, 4000, 30) {
        Ok(e) => e,
        Err(e) => {
            println!("new_dma failed: {e}");
            return;
        }
    };
    println!("opened DMA-BUF zero-copy pipeline at {w}x{h}");

    let phys: Size<i32, Physical> = (w as i32, h as i32).into();
    let mut total = 0usize;
    for f in 0..10u32 {
        let mut render =
            |r: &mut GlesRenderer, fb: &mut smithay::backend::renderer::gles::GlesTarget<'_>| {
                let mut frame = r
                    .render(fb, phys, Transform::Normal)
                    .map_err(|e| CompositorError::Renderer(format!("render: {e}")))?;
                let c = Color32F::new(0.1 * (f % 8) as f32, 0.4, 0.6, 1.0);
                frame
                    .clear(c, &[Rectangle::from_size(phys)])
                    .map_err(|e| CompositorError::Renderer(format!("clear: {e}")))?;
                let _sync = frame
                    .finish()
                    .map_err(|e| CompositorError::Renderer(format!("finish: {e}")))?;
                Ok(())
            };
        let frame = match dma.capture(&mut renderer, &mut render) {
            Ok(fr) => fr,
            Err(e) => {
                println!("frame {f}: capture error: {e}");
                return;
            }
        };
        if f == 5 {
            enc.force_idr_next();
        }
        match enc.submit(frame) {
            Ok(Some(nal)) => {
                total += nal.len();
                println!("frame {f}: {} bytes", nal.len());
            }
            Ok(None) => println!("frame {f}: (buffered)"),
            Err(e) => {
                println!("frame {f}: ENCODE ERROR: {e}");
                return;
            }
        }
    }
    println!("done — {total} total Annex-B bytes over 10 zero-copy frames");
}
