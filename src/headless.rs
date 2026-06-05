use std::time::Duration;

use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{
            Bind, Offscreen,
            damage::OutputDamageTracker,
            element::surface::WaylandSurfaceRenderElement,
            gles::{GlesRenderbuffer, GlesRenderer},
        },
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::{EventLoop, timer::{TimeoutAction, Timer}},
    utils::{Buffer, Size, Transform},
};

use crate::{
    Wado,
    capture::mem::capture_frame,
    conf::{SinkTarget, WadoConfig},
    encode::x264enc::X264Encoder,
    sink::{FrameSink, file::FileSink, webrtc::WebRtcSink},
};

/// Backward-compat aliases — prefer `WadoConfig::default().encoder.*` in new code.
pub const WIDTH: u32 = crate::conf::DEFAULT_WIDTH;
pub const HEIGHT: u32 = crate::conf::DEFAULT_HEIGHT;
pub const FPS: u32 = crate::conf::DEFAULT_FPS;

pub fn init_headless(
    event_loop: &mut EventLoop<Wado>,
    state: &mut Wado,
    config: &WadoConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    config.print_summary();

    let ec = &config.encoder;

    // ── EGL / GLES renderer ───────────────────────────────────────────────────
    let egl_display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay)? };
    let egl_context = EGLContext::new(&egl_display)?;
    let mut renderer = unsafe { GlesRenderer::new(egl_context)? };

    // ── Offscreen render target ───────────────────────────────────────────────
    let buf_size: Size<i32, Buffer> = (ec.width as i32, ec.height as i32).into();
    let renderbuffer: GlesRenderbuffer = renderer.create_buffer(Fourcc::Abgr8888, buf_size)?;

    // ── Logical Output (no physical display) ─────────────────────────────────
    let mode = Mode {
        size: (ec.width as i32, ec.height as i32).into(),
        refresh: (ec.fps * 1000) as i32,
    };
    let output = Output::new(
        "HEADLESS-1".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "wado".into(),
            model: "Headless".into(),
            serial_number: "0".into(),
        },
    );
    let _global = output.create_global::<Wado>(&state.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((0, 0).into()));
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    let damage_tracker = OutputDamageTracker::from_output(&output);

    // ── Encoder ───────────────────────────────────────────────────────────────
    let encoder = X264Encoder::new(
        ec.width,
        ec.height,
        ec.fps,
        ec.bitrate_kbps,
        ec.keyframe_interval,
        ec.preset,
    )?;

    // ── Sink ──────────────────────────────────────────────────────────────────
    // No explicit header send here — every IDR frame already carries SPS+PPS inline
    // (Fix 2 in x264enc). A late-joining client will sync on the next IDR.
    let sink: Box<dyn FrameSink> = match &config.output.sink {
        SinkTarget::WebRtc { http_addr } => Box::new(WebRtcSink::new(http_addr, ec.fps)?),
        SinkTarget::File(path) => Box::new(FileSink::create(path)?),
    };

    state.renderer = Some(renderer);
    state.renderbuffer = Some(renderbuffer);
    state.damage_tracker = Some(damage_tracker);
    state.encoder = Some(encoder);
    state.frame_sink = Some(sink);

    // ── 60 fps render timer ───────────────────────────────────────────────────
    let (w, h) = (ec.width, ec.height);
    let frame_nanos = 1_000_000_000 / ec.fps as u64;
    event_loop
        .handle()
        .insert_source(Timer::immediate(), move |deadline, _, state: &mut Wado| {
            if let Err(e) = render_tick(state, &output, w, h) {
                eprintln!("render_tick error: {e}");
            }
            TimeoutAction::ToInstant(deadline + Duration::from_nanos(frame_nanos))
        })?;

    Ok(())
}

fn render_tick(
    state: &mut Wado,
    output: &Output,
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let renderer = state.renderer.as_mut().unwrap();
    let renderbuffer = state.renderbuffer.as_mut().unwrap();
    let damage_tracker = state.damage_tracker.as_mut().unwrap();

    let mut fb = renderer.bind(renderbuffer)?;

    smithay::desktop::space::render_output::<
        _,
        WaylandSurfaceRenderElement<GlesRenderer>,
        _,
        _,
    >(
        output,
        renderer,
        &mut fb,
        1.0,
        0,
        [&state.space],
        &[],
        damage_tracker,
        [0.1, 0.1, 0.1, 1.0],
    )?;

    state.space.elements().for_each(|window| {
        window.send_frame(
            output,
            state.start_time.elapsed(),
            Some(Duration::ZERO),
            |_, _| Some(output.clone()),
        )
    });
    state.space.refresh();
    state.popups.cleanup();
    let _ = state.display_handle.flush_clients();

    let buf_size: Size<i32, Buffer> = (width as i32, height as i32).into();
    let pixels = capture_frame(renderer, &fb, buf_size)?;

    let encoder = state.encoder.as_mut().unwrap();
    if let Some(nal_bytes) = encoder.encode_rgba(&pixels) {
        if let Some(sink) = state.frame_sink.as_mut() {
            sink.send(&nal_bytes);
        }
    }

    Ok(())
}
