use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use smithay::{
    backend::{
        egl::EGLContext,
        renderer::{
            damage::OutputDamageTracker,
            element::surface::WaylandSurfaceRenderElement,
            gles::{GlesRenderer, GlesTarget},
        },
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::timer::{TimeoutAction, Timer},
    utils::{Buffer, Size, Transform},
};

use tracing::{debug, info, warn};

use wado_protocol::EncoderReport;

use crate::{
    Wado, CompositorError,
    capture::{CaptureTarget, DmaTarget, MemTarget, gpu, gpu::Gbm},
    pacing::TickStats,
    conf::{EncoderConfig, SinkTarget, WadoConfig},
    encode::{
        encoder::VideoEncoder,
        ffmpeg::{FfmpegVaapiEncoder, hwcontext::first_render_node},
        select::{Tier, tiers_for},
        x264enc::X264Encoder,
    },
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

/// Tell every currently-mapped surface the output scale it should draw for. Returns how many
/// were told, which is only interesting as a trace.
fn push_fractional_scale(state: &Wado, scale: f32) -> usize {
    use smithay::wayland::compositor::with_states;
    use smithay::wayland::fractional_scale::with_fractional_scale;

    let mut n = 0;
    for window in state.space.elements() {
        if let Some(surface) = window.toplevel().map(|t| t.wl_surface().clone()) {
            with_states(&surface, |states| {
                with_fractional_scale(states, |fs| fs.set_preferred_scale(scale as f64));
            });
            n += 1;
        }
    }
    n
}

/// Eager, standalone setup for the examples: build the configured sink and start a
/// session immediately. The live path uses `website` + `start_session` instead.
pub fn init_headless(state: &mut Wado, config: &WadoConfig) -> crate::Result<()> {
    config.print_summary();
    let sink: Box<dyn FrameSink> = match &config.output.sink {
        SinkTarget::File(path) => Box::new(FileSink::create(path)?),
    };
    // The examples render to a file with no client to scale for; 1.0 is the whole story.
    start_session(state, &config.encoder, 1.0, sink)?;
    Ok(())
}

/// Bring up the headless render pipeline for one session: EGL/GLES renderer, an
/// offscreen target, a client-sized `Output`, the encoder, and the render timer.
/// Stores everything on `state` and marks the session active. Does NOT launch the
/// session's application — the caller does that (see `spawn_session_command`).
pub fn start_session(
    state: &mut Wado,
    ec: &EncoderConfig,
    scale: f32,
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
    // Scale is what makes a desktop app usable on a phone-sized output: the mode stays at the
    // encoded pixel size, but the logical area clients lay out in shrinks by this factor, so
    // everything is drawn proportionally larger. Clamped because a zero or negative scale is
    // a divide-by-zero in the logical geometry, not a preference.
    let scale = if scale.is_finite() { scale.clamp(1.0, 4.0) } else { 1.0 };
    // Two audiences, two answers, which is exactly what `Scale::Custom` is for.
    //
    // `wl_output.scale` is an integer event, so a client that speaks only that has to be
    // told a whole number. A client that speaks wp-fractional-scale-v1 — which wado does
    // advertise, see `state.rs` — is told the real value and draws for it.
    //
    // The trap `Scale::Fractional` sets is that it rounds *up* for the integer protocols:
    // 1.25 is advertised as 2, the client draws a 2x buffer, and it is composited as 1.25x,
    // so the buffer overhangs its own area and app elements are clipped. That was the real
    // bug behind the old blanket rounding. Rounding to nearest instead means a legacy client
    // asked for 1.25 is told 1 and comes out slightly soft — wrong in the safe direction,
    // and only for clients that could not have honoured the request anyway.
    let advertised_integer = (scale.round() as i32).max(1);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(smithay::output::Scale::Custom {
            advertised_integer,
            fractional: scale as f64,
        }),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    // The fractional-scale handler answers a surface that asks, and a surface asks once when
    // it binds. Anything already mapped when a session starts — every window carried over
    // from a previous session in this compositor — bound before this output existed, and
    // would otherwise keep drawing for the old scale forever.
    let n = push_fractional_scale(state, scale);
    if n > 0 {
        tracing::debug!(surfaces = n, scale, "pushed fractional scale to existing surfaces");
    }

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

    // ── Render timer ──────────────────────────────────────────────────────────
    let frame_nanos = 1_000_000_000 / ec.fps.max(1) as u64;
    let frame_period = Duration::from_nanos(frame_nanos);
    let mut pacing = TickStats::new(ec.fps);
    let token = state
        .loop_handle
        .insert_source(
            Timer::immediate(),
            move |deadline, _, state: &mut Wado| {
                pacing.tick();
                // Guard the render path: a panic here must tear down only the session,
                // not abort the process (and the server with it). Only Rust unwinding
                // panics are caught — a native segfault in EGL/GLES/x264 still aborts.
                match std::panic::catch_unwind(AssertUnwindSafe(|| render_tick(state))) {
                    Ok(Ok(())) => {}
                    // Transient per-frame failure: log and keep the timer alive so
                    // the pipeline can recover on the next tick.
                    Ok(Err(e)) => tracing::warn!("render tick failed: {e}"),
                    Err(_) => {
                        tracing::error!("compositor panicked during render — stopping session");
                        // We're inside this timer's own callback: take the token so
                        // stop_session won't `remove()` the source we're about to drop
                        // via the return value below (avoids a double-remove).
                        let _ = state.render_timer_token.take();
                        stop_session(state);
                        return TimeoutAction::Drop;
                    }
                }
                // Pace the next tick WITHOUT accumulating lateness. `deadline` is the
                // instant this tick was *scheduled* for, not now, so the naive
                // `deadline + frame_period` permanently falls behind wall clock as soon
                // as one tick overruns its budget: every later deadline is already in
                // the past, the timer is always ready, calloop never idles, and remote
                // input/Wayland/control sources starve behind a busy render loop. It is
                // a latching failure — it never recovers on its own.
                //
                // On time: keep the original phase, so pacing stays drift-free.
                // Behind: drop the missed frames and re-phase from now, which restores
                // the idle gap other event sources need.
                // ponytail: drops late frames rather than rendering them; fine for a live
                // stream where only the newest frame matters. Revisit only if we ever need
                // a recorded, gap-free capture.
                let next = deadline + frame_period;
                let now = Instant::now();
                TimeoutAction::ToInstant(if next <= now { now + frame_period } else { next })
            },
        )
        .map_err(|e| CompositorError::Other(format!("insert render timer: {e}")))?;
    state.render_timer_token = Some(token);

    // Every knob, not just the three that were here. A drop or latency warning is only
    // actionable next to the settings that produced it, and `Quality::Balanced` in the
    // request says nothing — the derived CBR target is the number that matters.
    info!(
        width = ec.width,
        height = ec.height,
        fps = ec.fps,
        bitrate_kbps = ec.bitrate_kbps,
        keyframe_interval = ec.keyframe_interval,
        preset = ?ec.preset,
        backend = ?ec.backend,
        scale,
        "compositor session active"
    );
    Ok(encoder_report)
}

/// Launch a command (free-form, space-split into program + args) into the session and
/// remember the child so `stop_session` can kill it. Callable both at session start
/// (the optional initial command) and at runtime — any number of times, so a session
/// can host many apps. `WAYLAND_DISPLAY` is already set process-wide by `build`.
pub fn launch_command(state: &mut Wado, command: &str) {
    if command.trim().is_empty() {
        warn!("empty command — nothing to launch");
        return;
    }
    // Through a shell and in its own process group — see `proc::spawn`.
    //
    // This grants no access that did not exist: the field was already free-form and spawned
    // whatever it named. It is still the reason the direct-mode control plane binds
    // localhost, and the reason the relay's Remote ID is the only thing between a stranger
    // and this shell — see the auth gate in TODO's NECESSARY list.
    match crate::proc::spawn(command) {
        Ok(child) => {
            info!(pid = child.id(), command, "launched session application");
            state.app_processes.push(child);
        }
        // Not fatal to the session: the stream still runs, the window is just empty.
        Err(e) => tracing::error!("failed to launch session app {command:?}: {e}"),
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
    // Whole groups, not single pids: `child.kill()` reaped the shell and left everything it
    // had forked running — a browser kept playing audio after the session it belonged to was
    // gone. See `proc::terminate`.
    for mut child in state.app_processes.drain(..) {
        crate::proc::terminate(&mut child);
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
            )?;
            let cap = MemTarget::new(renderer, buf_size)?;
            Ok((Box::new(enc), Box::new(cap)))
        }
        Tier::VaapiDma => {
            let gbm = gbm
                .clone()
                .ok_or_else(|| CompositorError::Encoder("DMA tier needs a GBM device".into()))?;
            let enc = FfmpegVaapiEncoder::new_dma(
                &node()?,
                ec.width,
                ec.height,
                ec.fps,
                ec.bitrate_kbps,
                ec.keyframe_interval,
            )?;
            let cap = DmaTarget::new(gbm, buf_size)?;
            Ok((Box::new(enc), Box::new(cap)))
        }
    }
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
            Ok((enc, cap)) => return Ok((enc, cap, tier.report(), tier)),
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
                state.encoder = Some(enc);
                state.capture = Some(cap);
                state.current_tier = Some(tier);
                if let Some(e) = state.encoder.as_mut() {
                    e.force_idr_next();
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
    let tick_start = Instant::now();

    // The block yields the encode result plus how long each stage took, so neither
    // duration needs a dummy initial value.
    let (result, capture_dur, encode_dur): (crate::Result<Option<Vec<u8>>>, Duration, Duration) = {
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

        // Timed separately so the client's breakdown can say WHICH stage costs the time:
        // a slow capture points at the GL/readback path, a slow encode at the encoder tier.
        match capture.capture(renderer, &mut render) {
            Ok(frame) => {
                let captured = tick_start.elapsed();
                let encode_start = Instant::now();
                let out = state.encoder.as_mut().unwrap().submit(frame);
                (out, captured, encode_start.elapsed())
            }
            Err(e) => (Err(e), tick_start.elapsed(), Duration::ZERO),
        }
    };

    match result {
        Ok(Some(nal_bytes)) => {
            if let Some(sink) = state.frame_sink.as_mut() {
                sink.send(&nal_bytes);
            }
        }
        Ok(None) => {}
        // Robustness: a runtime encode failure downgrades the pipeline one tier instead of
        // failing every frame. The render loop continues; the next tick uses the new tier.
        Err(e) => {
            warn!("encode failed on tier {:?}: {e} — downgrading", state.current_tier);
            downgrade_pipeline(state);
        }
    }

    if let Some(timing) = state.timing.as_mut() {
        // `queue` is measured by the pump (it is the only side that knows when the frame
        // was taken), so it is not passed here — see `StageTimings::queue_ms`.
        timing.frame(tick_start, capture_dur, encode_dur, Duration::ZERO);
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
