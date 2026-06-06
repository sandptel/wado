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
    reexports::calloop::timer::{TimeoutAction, Timer},
    utils::{Buffer, Size, Transform},
};

use tracing::{debug, info, warn};

use crate::{
    Wado, WadoError,
    capture::mem::capture_frame,
    conf::{EncoderConfig, SinkTarget, WadoConfig},
    encode::x264enc::X264Encoder,
    sink::{FrameSink, file::FileSink},
};

/// Map any displayable renderer/EGL error into [`WadoError::Renderer`].
fn renderer_err<E: std::fmt::Display>(ctx: &str) -> impl FnOnce(E) -> WadoError + '_ {
    move |e| WadoError::Renderer(format!("{ctx}: {e}"))
}

/// Backward-compat aliases — prefer `WadoConfig::default().encoder.*` in new code.
pub const WIDTH: u32 = crate::conf::DEFAULT_WIDTH;
pub const HEIGHT: u32 = crate::conf::DEFAULT_HEIGHT;
pub const FPS: u32 = crate::conf::DEFAULT_FPS;

/// Eager, standalone setup for the examples: build the configured sink and start a
/// session immediately. The live path uses `website` + `start_session` instead.
pub fn init_headless(state: &mut Wado, config: &WadoConfig) -> crate::Result<()> {
    config.print_summary();
    let sink: Box<dyn FrameSink> = match &config.output.sink {
        SinkTarget::File(path) => Box::new(FileSink::create(path)?),
    };
    start_session(state, &config.encoder, sink)
}

/// Bring up the headless render pipeline for one session: EGL/GLES renderer, an
/// offscreen target, a client-sized `Output`, the encoder, and the render timer.
/// Stores everything on `state` and marks the session active. Does NOT launch the
/// session's application — the caller does that (see `spawn_session_command`).
pub fn start_session(
    state: &mut Wado,
    ec: &EncoderConfig,
    sink: Box<dyn FrameSink>,
) -> crate::Result<()> {
    if state.session_active {
        return Err(WadoError::SessionAlreadyActive);
    }
    debug!(
        width = ec.width,
        height = ec.height,
        fps = ec.fps,
        bitrate_kbps = ec.bitrate_kbps,
        "starting compositor session — building EGL/GLES + encoder"
    );

    // ── EGL / GLES renderer ───────────────────────────────────────────────────
    let egl_display =
        unsafe { EGLDisplay::new(EGLSurfacelessDisplay).map_err(renderer_err("EGLDisplay::new"))? };
    let egl_context = EGLContext::new(&egl_display).map_err(renderer_err("EGLContext::new"))?;
    let mut renderer =
        unsafe { GlesRenderer::new(egl_context).map_err(renderer_err("GlesRenderer::new"))? };

    // ── Offscreen render target ───────────────────────────────────────────────
    let buf_size: Size<i32, Buffer> = (ec.width as i32, ec.height as i32).into();
    let renderbuffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, buf_size)
        .map_err(renderer_err("create_buffer"))?;

    // ── Logical Output (no physical display), sized to the client ─────────────
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
    let global = output.create_global::<Wado>(&state.display_handle);
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

    state.renderer = Some(renderer);
    state.renderbuffer = Some(renderbuffer);
    state.damage_tracker = Some(damage_tracker);
    state.encoder = Some(encoder);
    state.frame_sink = Some(sink);
    state.output = Some(output);
    state.output_global = Some(global);
    state.session_active = true;

    // ── Render timer ──────────────────────────────────────────────────────────
    let (w, h) = (ec.width, ec.height);
    let frame_nanos = 1_000_000_000 / ec.fps.max(1) as u64;
    let token = state
        .loop_handle
        .insert_source(
            Timer::immediate(),
            move |deadline, _, state: &mut Wado| {
                if let Err(e) = render_tick(state, w, h) {
                    // Transient per-frame failure: log and keep the timer alive so
                    // the pipeline can recover on the next tick.
                    tracing::warn!("render tick failed: {e}");
                }
                TimeoutAction::ToInstant(deadline + Duration::from_nanos(frame_nanos))
            },
        )
        .map_err(|e| WadoError::Other(format!("insert render timer: {e}")))?;
    state.render_timer_token = Some(token);

    info!(width = ec.width, height = ec.height, fps = ec.fps, "compositor session active");
    Ok(())
}

/// Spawn the session's application (free-form command, space-split into program +
/// args) and remember the child so `stop_session` can kill it. `WAYLAND_DISPLAY` is
/// already set process-wide in `main`.
pub fn spawn_session_command(state: &mut Wado, command: &str) {
    let mut parts = command.split_whitespace();
    let Some(program) = parts.next() else {
        warn!("empty session command — nothing to launch");
        return;
    };
    let args: Vec<&str> = parts.collect();
    match std::process::Command::new(program).args(&args).spawn() {
        Ok(child) => {
            info!(pid = child.id(), command, "launched session application");
            state.app_process = Some(child);
        }
        // Not fatal to the session: the stream still runs, the window is just empty.
        Err(e) => tracing::error!("failed to launch session app {program:?}: {e}"),
    }
}

/// Tear down the active session and free its resources. Idempotent.
pub fn stop_session(state: &mut Wado) {
    if !state.session_active {
        return;
    }

    if let Some(token) = state.render_timer_token.take() {
        state.loop_handle.remove(token);
    }
    if let Some(mut child) = state.app_process.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(output) = state.output.take() {
        state.space.unmap_output(&output);
    }
    if let Some(global) = state.output_global.take() {
        state.display_handle.remove_global::<Wado>(global);
    }
    state.renderer = None;
    state.renderbuffer = None;
    state.damage_tracker = None;
    state.encoder = None;
    state.frame_sink = None;
    state.session_active = false;

    info!("compositor session stopped — resources released");
}

/// Request that the next encoded frame be a forced IDR keyframe. Called when a new
/// viewer connects or the browser sends a PLI/FIR (picture loss). No-op if idle.
pub fn force_keyframe(state: &mut Wado) {
    if let Some(encoder) = state.encoder.as_mut() {
        encoder.force_idr_next();
        debug!("forced IDR keyframe requested");
    }
}

fn render_tick(state: &mut Wado, width: u32, height: u32) -> crate::Result<()> {
    if !state.session_active {
        return Ok(());
    }
    let Some(output) = state.output.clone() else {
        return Ok(());
    };

    let renderer = state.renderer.as_mut().unwrap();
    let renderbuffer = state.renderbuffer.as_mut().unwrap();
    let damage_tracker = state.damage_tracker.as_mut().unwrap();

    let mut fb = renderer.bind(renderbuffer).map_err(renderer_err("bind"))?;

    smithay::desktop::space::render_output::<
        _,
        WaylandSurfaceRenderElement<GlesRenderer>,
        _,
        _,
    >(
        &output,
        renderer,
        &mut fb,
        1.0,
        0,
        [&state.space],
        &[],
        damage_tracker,
        [0.1, 0.1, 0.1, 1.0],
    )
    .map_err(renderer_err("render_output"))?;

    state.space.elements().for_each(|window| {
        window.send_frame(
            &output,
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
