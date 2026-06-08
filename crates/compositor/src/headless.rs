use std::panic::AssertUnwindSafe;
use std::time::Duration;

use smithay::{
    backend::{
        allocator::{Fourcc, Modifier},
        egl::EGLContext,
        renderer::{
            Color32F, Frame as _, Renderer as _,
            damage::OutputDamageTracker,
            element::surface::WaylandSurfaceRenderElement,
            gles::{GlesRenderer, GlesTarget},
        },
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::timer::{TimeoutAction, Timer},
    utils::{Buffer, Physical, Rectangle, Size, Transform},
};

use tracing::{debug, info, warn};

use wado_protocol::EncoderReport;

use crate::{
    Wado, CompositorError,
    capture::{CaptureTarget, DmaTarget, MemTarget, gpu, gpu::Gbm},
    conf::{EncoderConfig, SinkTarget, WadoConfig},
    encode::{
        encoder::VideoEncoder,
        ffmpeg::{FfmpegVaapiEncoder, hwcontext::first_render_node},
        select::{Tier, tiers_for},
        stall_watchdog::StallWatchdog,
        x264enc::X264Encoder,
    },
    event::SessionEvent,
    sink::{FrameSink, file::FileSink},
};

/// Map any displayable renderer/EGL error into [`CompositorError::Renderer`].
fn renderer_err<E: std::fmt::Display>(ctx: &str) -> impl FnOnce(E) -> CompositorError + '_ {
    move |e| CompositorError::Renderer(format!("{ctx}: {e}"))
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
    start_session(state, &config.encoder, sink)?;
    Ok(())
}

/// Bring up the headless render pipeline for one session: EGL/GLES renderer, an
/// offscreen target, a client-sized `Output`, the encoder, and the render timer.
/// Stores everything on `state` and marks the session active. Does NOT launch the
/// session's application — the caller does that (see `spawn_session_command`).
pub fn start_session(
    state: &mut Wado,
    ec: &EncoderConfig,
    sink: Box<dyn FrameSink>,
) -> crate::Result<EncoderReport> {
    if state.session_active {
        return Err(CompositorError::SessionAlreadyActive);
    }
    debug!(
        width = ec.width,
        height = ec.height,
        fps = ec.fps,
        bitrate_kbps = ec.bitrate_kbps,
        "starting compositor session — building EGL/GLES + encoder"
    );

    // ── GPU: GBM device + EGL (dmabuf-capable), or surfaceless fallback ───────
    let gpu = gpu::open()?;
    let egl_context = EGLContext::new(&gpu.egl).map_err(renderer_err("EGLContext::new"))?;
    let mut renderer =
        unsafe { GlesRenderer::new(egl_context).map_err(renderer_err("GlesRenderer::new"))? };

    let buf_size: Size<i32, Buffer> = (ec.width as i32, ec.height as i32).into();

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

    // ── Pipeline tier (zero-copy DMA-BUF → CPU-upload VAAPI → x264) ───────────
    // Tried top-down; the first that opens wins. Building the tier *is* the probe
    // (invariant #6 — we actually open the encoder + capture target).
    let (encoder, capture, encoder_report, tier) =
        build_pipeline(&mut renderer, &gpu.gbm, ec, buf_size)?;
    info!(
        ?tier,
        pipeline = %encoder_report.pipeline,
        "pipeline selected"
    );

    state.renderer = Some(renderer);
    state.gbm = gpu.gbm;
    state.capture = Some(capture);
    state.damage_tracker = Some(damage_tracker);
    state.encoder = Some(encoder);
    state.current_tier = Some(tier);
    state.encoder_config = Some(ec.clone());
    state.frame_sink = Some(sink);
    state.output = Some(output);
    state.output_global = Some(global);
    state.session_active = true;
    // Arm the stall watchdog calibrated to the session's frame rate.
    state.stall_watchdog = Some(StallWatchdog::new(ec.fps));

    // ── Render timer ──────────────────────────────────────────────────────────
    let frame_nanos = 1_000_000_000 / ec.fps.max(1) as u64;
    let token = state
        .loop_handle
        .insert_source(
            Timer::immediate(),
            move |deadline, _, state: &mut Wado| {
                // Guard the render path: a panic here must tear down only the session,
                // not abort the process (and the server with it). Only Rust unwinding
                // panics are caught — a native segfault in EGL/GLES/x264 still aborts.
                match std::panic::catch_unwind(AssertUnwindSafe(|| render_tick(state))) {
                    Ok(Ok(())) => {}
                    // Transient per-frame failure: log and keep the timer alive so
                    // the pipeline can recover on the next tick.
                    Ok(Err(e)) => tracing::warn!("render tick failed: {e}"),
                    Err(payload) => {
                        // Extract a human-readable message from the panic payload.
                        let msg = payload
                            .downcast_ref::<&str>()
                            .copied()
                            .or_else(|| {
                                payload.downcast_ref::<String>().map(String::as_str)
                            })
                            .unwrap_or("<opaque panic value>");
                        tracing::error!(
                            "compositor panicked during render: {msg} — stopping session"
                        );
                        // Notify connected clients before tearing down, so they can
                        // show a reconnect banner instead of a silent frozen video.
                        if let Some(tx) = &state.event_tx {
                            let _ = tx.try_send(SessionEvent::SessionEnded {
                                reason: msg.to_string(),
                            });
                        }
                        // We're inside this timer's own callback: take the token so
                        // stop_session won't `remove()` the source we're about to drop
                        // via the return value below (avoids a double-remove).
                        let _ = state.render_timer_token.take();
                        stop_session(state);
                        return TimeoutAction::Drop;
                    }
                }
                TimeoutAction::ToInstant(deadline + Duration::from_nanos(frame_nanos))
            },
        )
        .map_err(|e| CompositorError::Other(format!("insert render timer: {e}")))?;
    state.render_timer_token = Some(token);

    info!(width = ec.width, height = ec.height, fps = ec.fps, "compositor session active");
    Ok(encoder_report)
}

/// Launch a command (free-form, space-split into program + args) into the session and
/// remember the child so `stop_session` can kill it. Callable both at session start
/// (the optional initial command) and at runtime — any number of times, so a session
/// can host many apps. `WAYLAND_DISPLAY` is already set process-wide by `build`.
pub fn launch_command(state: &mut Wado, command: &str) {
    let mut parts = command.split_whitespace();
    let Some(program) = parts.next() else {
        warn!("empty command — nothing to launch");
        return;
    };
    let args: Vec<&str> = parts.collect();
    match std::process::Command::new(program).args(&args).spawn() {
        Ok(child) => {
            info!(pid = child.id(), command, "launched session application");
            state.app_processes.push(child);
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
    for mut child in state.app_processes.drain(..) {
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
    state.capture = None;
    state.gbm = None;
    state.damage_tracker = None;
    state.encoder = None;
    state.current_tier = None;
    state.encoder_config = None;
    state.frame_sink = None;
    state.session_active = false;
    state.stall_watchdog = None;
    state.window_move = None;
    state.pending_placement.clear();
    state.cascade_count = 0;

    info!("compositor session stopped — resources released");
}

/// Best-effort CPU snapshot of the last rendered frame in `Abgr8888`, for debug tooling
/// (e.g. `examples/capture_to_disk.rs` PPM snapshots). `None` when idle or when the active
/// capture target can't cheaply read back to the CPU (e.g. a DMA-BUF target).
pub fn snapshot_rgba(state: &mut Wado) -> Option<Vec<u8>> {
    let renderer = state.renderer.as_mut()?;
    let capture = state.capture.as_mut()?;
    match capture.current_rgba(renderer) {
        Ok(px) => px,
        Err(e) => {
            warn!("snapshot_rgba failed: {e}");
            None
        }
    }
}

/// Build the encoder + capture target for one pipeline tier. Building it *is* the probe
/// (invariant #6): a failure here means the tier is unavailable and the caller tries the
/// next one. `gbm` is required for the DMA tier.
fn build_tier(
    tier: Tier,
    renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
    gbm: &Option<Gbm>,
    ec: &EncoderConfig,
    buf_size: Size<i32, Buffer>,
) -> crate::Result<(Box<dyn VideoEncoder>, Box<dyn CaptureTarget>)> {
    let node = || {
        first_render_node()
            .ok_or_else(|| CompositorError::Encoder("no DRM render node".into()))
    };
    match tier {
        Tier::X264 => {
            let enc = X264Encoder::new(
                ec.width,
                ec.height,
                ec.fps,
                ec.bitrate_kbps,
                ec.keyframe_interval,
                ec.keyframe_mode,
                ec.preset,
            )?;
            let cap = MemTarget::new(renderer, buf_size)?;
            Ok((Box::new(enc), Box::new(cap)))
        }
        Tier::VaapiCpu => {
            let enc = FfmpegVaapiEncoder::new(
                &node()?,
                ec.width,
                ec.height,
                ec.fps,
                ec.bitrate_kbps,
                ec.keyframe_interval,
                ec.keyframe_mode,
            )?;
            let cap = MemTarget::new(renderer, buf_size)?;
            Ok((Box::new(enc), Box::new(cap)))
        }
        Tier::VaapiDma => {
            let gbm = gbm
                .clone()
                .ok_or_else(|| CompositorError::Encoder("DMA tier needs a GBM device".into()))?;
            let mut enc = FfmpegVaapiEncoder::new_dma(
                &node()?,
                ec.width,
                ec.height,
                ec.fps,
                ec.bitrate_kbps,
                ec.keyframe_interval,
                ec.keyframe_mode,
            )?;

            // Query GPU-preferred non-Linear modifiers for Abgr8888 render targets.
            // EGL advertises which tiled modifiers the driver prefers; we verify them
            // end-to-end (GBM alloc → GLES render → VAAPI DRM-PRIME import + encode)
            // before committing — building the tier *is* the probe (invariant #6).
            let preferred: Vec<Modifier> = renderer
                .egl_context()
                .dmabuf_render_formats()
                .iter()
                .filter(|f| f.code == Fourcc::Abgr8888 && f.modifier != Modifier::Linear)
                .map(|f| f.modifier)
                .collect();

            let tried_tiled = !preferred.is_empty();
            if tried_tiled {
                // Pass tiled modifiers first so GBM picks the best one; Linear is the
                // swapchain-level fallback if GBM can't allocate tiled.
                let mut mods = preferred;
                mods.push(Modifier::Linear);
                match DmaTarget::new(gbm.clone(), buf_size, mods) {
                    Ok(mut cap) => {
                        match self_test_dma(renderer, &mut cap, &mut enc, buf_size) {
                            Ok(()) => {
                                enc.force_idr_next(); // probe consumed first IDR
                                tracing::info!("Tier A: tiled modifier verified end-to-end");
                                return Ok((Box::new(enc), Box::new(cap)));
                            }
                            Err(e) => tracing::warn!(
                                "Tier A: tiled modifier self-test failed ({e}), retrying with Linear"
                            ),
                        }
                    }
                    Err(e) => tracing::warn!(
                        "Tier A: tiled modifier alloc failed ({e}), retrying with Linear"
                    ),
                }
            }

            // Linear path: status-quo or tiled fallback. Self-test only when we already
            // tried tiled (guard against a regression introduced by the query path itself;
            // proven path stays unchanged when no tiled modifiers were advertised).
            let mut cap = DmaTarget::new(gbm, buf_size, vec![Modifier::Linear])?;
            if tried_tiled {
                self_test_dma(renderer, &mut cap, &mut enc, buf_size)?;
                enc.force_idr_next();
                tracing::info!("Tier A: Linear modifier (tiled fallback) verified");
            }
            Ok((Box::new(enc), Box::new(cap)))
        }
    }
}

/// One clear→capture→submit cycle to verify the DMA-BUF modifier end-to-end.
/// Returns `Ok(())` if all three steps succeed. The encode output is discarded
/// (the caller resets the encoder with `force_idr_next` after success).
fn self_test_dma(
    renderer: &mut GlesRenderer,
    capture: &mut DmaTarget,
    encoder: &mut dyn VideoEncoder,
    size: Size<i32, Buffer>,
) -> crate::Result<()> {
    let phys: Size<i32, Physical> = (size.w, size.h).into();
    let mut render = |r: &mut GlesRenderer, fb: &mut GlesTarget<'_>| -> crate::Result<()> {
        let mut frame = r
            .render(fb, phys, Transform::Normal)
            .map_err(renderer_err("self_test render"))?;
        frame
            .clear(Color32F::new(0.0, 0.0, 0.0, 1.0), &[Rectangle::from_size(phys)])
            .map_err(renderer_err("self_test clear"))?;
        let _sync = frame.finish().map_err(renderer_err("self_test finish"))?;
        Ok(())
    };
    let frame = capture.capture(renderer, &mut render)?;
    encoder.submit(frame).map(|_| ())
}

/// Pick the best pipeline tier that actually opens for the session's backend preference.
fn build_pipeline(
    renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
    gbm: &Option<Gbm>,
    ec: &EncoderConfig,
    buf_size: Size<i32, Buffer>,
) -> crate::Result<(Box<dyn VideoEncoder>, Box<dyn CaptureTarget>, wado_protocol::EncoderReport, Tier)>
{
    let mut last_err = None;
    for tier in tiers_for(ec.backend, gbm.is_some()) {
        match build_tier(tier, renderer, gbm, ec, buf_size) {
            Ok((enc, cap)) => return Ok((enc, cap, tier.report(ec.keyframe_mode), tier)),
            Err(e) => {
                warn!(?tier, "pipeline tier unavailable, trying next: {e}");
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| CompositorError::Encoder("no encoder tier available".into())))
}

/// Drop the active pipeline to the next tier down after a runtime encode failure
/// (downgrade-once, repeated until a tier works or the ladder is exhausted). Forces an IDR
/// so the new encoder's stream starts clean. The render loop keeps running throughout.
fn downgrade_pipeline(state: &mut Wado) {
    let Some(cur) = state.current_tier else { return };
    let Some(ec) = state.encoder_config.clone() else { return };
    let buf_size: Size<i32, Buffer> = (ec.width as i32, ec.height as i32).into();
    let Some(renderer) = state.renderer.as_mut() else { return };

    let mut next = cur.next();
    while let Some(tier) = next {
        match build_tier(tier, renderer, &state.gbm, &ec, buf_size) {
            Ok((enc, cap)) => {
                warn!(from = ?cur, to = ?tier, "pipeline downgraded after runtime encode failure");
                let new_report = tier.report(ec.keyframe_mode);
                state.encoder = Some(enc);
                state.capture = Some(cap);
                state.current_tier = Some(tier);
                if let Some(e) = state.encoder.as_mut() {
                    e.force_idr_next();
                }
                // Reset the stall counter so the new encoder starts with a clean slate.
                if let Some(w) = state.stall_watchdog.as_mut() {
                    w.reset();
                }
                if let Some(tx) = &state.event_tx {
                    let _ = tx.try_send(SessionEvent::EncoderChanged(new_report));
                }
                return;
            }
            Err(e) => {
                warn!(?tier, "downgrade tier also failed: {e}");
                next = tier.next();
            }
        }
    }
    tracing::error!("no fallback encoder tier left — session video is broken");
    // All tiers exhausted: notify the client so it shows a reconnect banner instead of
    // a silent frozen video (the session stays technically alive but is producing nothing).
    if let Some(tx) = &state.event_tx {
        let _ = tx.try_send(SessionEvent::SessionEnded {
            reason: "all encoder tiers exhausted".to_string(),
        });
    }
}

/// Request that the next encoded frame be a forced IDR keyframe. Called when a new
/// viewer connects or the browser sends a PLI/FIR (picture loss). No-op if idle.
pub fn force_keyframe(state: &mut Wado) {
    if let Some(encoder) = state.encoder.as_mut() {
        encoder.force_idr_next();
        debug!("forced IDR keyframe requested");
    }
}

fn render_tick(state: &mut Wado) -> crate::Result<()> {
    if !state.session_active {
        return Ok(());
    }
    let Some(output) = state.output.clone() else {
        return Ok(());
    };

    // Render into the active capture target and hand the resulting frame to the encoder.
    // The render itself is a closure so the capture target (`MemTarget`/`DmaTarget`) owns
    // the bind/export and the render tick stays capture-agnostic. Disjoint field borrows
    // (renderer / capture / damage_tracker / space / encoder) keep the borrow checker happy.
    // Returns the encoder result (owned) so the `frame` borrow on `capture` is released
    // before we may rebuild the pipeline on failure.
    let result: crate::Result<Option<Vec<u8>>> = {
        let renderer = state.renderer.as_mut().unwrap();
        let capture = state.capture.as_mut().unwrap();
        let damage_tracker = state.damage_tracker.as_mut().unwrap();
        let space = &state.space;
        let bg = [0.1, 0.1, 0.1, 1.0];

        let mut render = |r: &mut GlesRenderer, fb: &mut GlesTarget<'_>| -> crate::Result<()> {
            smithay::desktop::space::render_output::<
                _,
                WaylandSurfaceRenderElement<GlesRenderer>,
                _,
                _,
            >(&output, r, fb, 1.0, 0, [space], &[], damage_tracker, bg)
            .map_err(renderer_err("render_output"))?;
            Ok(())
        };

        match capture.capture(renderer, &mut render) {
            Ok(frame) => state.encoder.as_mut().unwrap().submit(frame),
            Err(e) => Err(e),
        }
    };

    match result {
        Ok(Some(nal_bytes)) => {
            // Got a packet: reset stall counter and forward to the network sink.
            if let Some(w) = state.stall_watchdog.as_mut() {
                w.reset();
            }
            if let Some(sink) = state.frame_sink.as_mut() {
                sink.send(&nal_bytes);
            }
        }
        Ok(None) => {
            // No packet this tick: count consecutive empties.
            // A VAAPI encoder under a very long GOP can silently return Ok(None) every
            // tick without error — the stall watchdog detects this and auto-downgrades.
            let stalled = state.stall_watchdog.as_mut().map_or(false, |w| w.tick(false));
            if stalled {
                let n = state
                    .stall_watchdog
                    .as_ref()
                    .map_or(0, |w| w.consecutive_empty());
                warn!(
                    tier = ?state.current_tier,
                    "encoder stalled: no packets for {n} consecutive ticks — downgrading pipeline"
                );
                // Reset before downgrade so the new encoder starts with a clean slate.
                if let Some(w) = state.stall_watchdog.as_mut() {
                    w.reset();
                }
                downgrade_pipeline(state);
            }
        }
        // Robustness: a runtime encode failure downgrades the pipeline one tier instead of
        // failing every frame. The render loop continues; the next tick uses the new tier.
        Err(e) => {
            warn!("encode failed on tier {:?}: {e} — downgrading", state.current_tier);
            downgrade_pipeline(state);
        }
    }

    // Post-frame bookkeeping (after the capture/encode borrows are released).
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

    Ok(())
}
