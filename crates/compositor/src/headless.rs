use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use smithay::{
    backend::{
        egl::EGLContext,
        renderer::{
            ImportDma,
            damage::OutputDamageTracker,
            element::surface::WaylandSurfaceRenderElement,
            gles::{GlesRenderer, GlesTarget},
        },
    },
    desktop::utils::OutputPresentationFeedback,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    reexports::wayland_server::backend::GlobalId,
    reexports::calloop::timer::{TimeoutAction, Timer},
    reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
    wayland::presentation::{PresentationFeedbackCachedState, Refresh},
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

    // ── zwp-linux-dmabuf-v1 ──────────────────────────────────────────────────
    //
    // Created here, not in `Wado::new`, because its format list comes from the renderer and
    // there is no renderer until now. Every Wayland client here is spawned by the compositor
    // into a running session, so none of them can bind before this point — the usual
    // "advertise unconditionally or a toolkit never looks again" rule has nothing to bite on.
    //
    // Version 4 (with feedback) when the device id is known, because feedback is how a client
    // is told *which* GPU to allocate on; version 3 (bare format list) otherwise, which is
    // still enough for a client to stop going through shm.
    // A start that fails below this point leaves `session_active` false, so `stop_session`
    // early-returns and never tears this down. Dropping any previous global here keeps the
    // invariant simple: at most one, always the current renderer's.
    state.dmabuf_logged = false;
    state.frame_seq = 0;
    state.presentation_logged = false;
    state.congestion.reset();
    // Belongs to the viewer that just left; a new one has not said anything yet.
    state.viewer_strained = false;
    state.viewer_attached = true;
    let _ = state.shedding_tx.send(1);
    state.content_type_log.clear();
    if let Some(old) = state.dmabuf_global.take() {
        state
            .dmabuf_state
            .destroy_global::<Wado>(&state.display_handle, old);
    }
    match gpu.dev {
        Some(dev) => {
            let formats: Vec<_> = renderer.dmabuf_formats().into_iter().collect();
            match smithay::wayland::dmabuf::DmabufFeedbackBuilder::new(dev, formats).build() {
                Ok(feedback) => {
                    state.dmabuf_global = Some(
                        state
                            .dmabuf_state
                            .create_global_with_default_feedback::<Wado>(
                                &state.display_handle,
                                &feedback,
                            ),
                    );
                    info!("zwp-linux-dmabuf-v1 advertised (v4, with feedback)");
                }
                Err(e) => warn!("dmabuf feedback build failed, clients stay on shm: {e}"),
            }
        }
        None => {
            let formats: Vec<_> = renderer.dmabuf_formats().into_iter().collect();
            let n = formats.len();
            state.dmabuf_global = Some(
                state
                    .dmabuf_state
                    .create_global::<Wado>(&state.display_handle, formats),
            );
            info!(
                formats = n,
                "zwp-linux-dmabuf-v1 advertised (v3, no device id)"
            );
        }
    }

    let buf_size: Size<i32, Buffer> = (ec.width as i32, ec.height as i32).into();
    let (output, global, damage_tracker) = build_output(state, ec, scale);
    let scale = clamp_scale(scale);
    state.output_scale = scale;

    // ── Pipeline tier (zero-copy DMA-BUF → CPU-upload VAAPI → x264) ───────────
    // Tried top-down; the first that opens wins. Building the tier *is* the probe
    // (invariant #6 — we actually open the encoder + capture target).
    let (encoder, capture, mut encoder_report, tier) =
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

    install_render_timer(state, ec.fps)?;

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
        // The one number that predicts whether a config will look starved, and nothing logged
        // it. Bitrate and fps are exposed as independent settings, so moving 60 -> 120 silently
        // halves the per-frame bit budget; `memory/latency/bandwidth.md` records 0.016 as
        // starvation. Logged here so every trace carries it and no comparison has to reconstruct
        // it from three other fields.
        bits_per_px = format!("{:.4}", bits_per_pixel(ec)),
        "compositor session active"
    );
    // What the encoder was actually built with, so the client can compare what arrives against
    // what was asked for. Set here rather than in `Tier::report()` because the tier does not
    // hold the resolved config.
    encoder_report.bitrate_kbps = ec.bitrate_kbps;
    encoder_report.fps = ec.fps;
    Ok(encoder_report)
}


/// A scale that cannot divide by zero. A zero or negative value is a bug, not a preference.
fn clamp_scale(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(1.0, 4.0) } else { 1.0 }
}

/// Build the session's `Output`, map it into the space, and return it with its global and a
/// fresh damage tracker.
///
/// Shared by `start_session` and `reconfigure_session`, and that sharing is the point:
/// **invariant #8 says a resolution comes from a *fresh* `Output`, never a mutated one** —
/// Wayland cannot un-advertise a mode — so changing the shape of a running session means
/// running exactly this code again.
fn build_output(
    state: &mut Wado,
    ec: &EncoderConfig,
    scale: f32,
) -> (Output, GlobalId, OutputDamageTracker) {
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
    // so the buffer overhangs its own area and app elements are clipped. That is the failure
    // `634c603` recorded as visible at 1.75 and 2.5, and it is the reason this is `floor`
    // rather than `round`: round and ceil are the same number at 1.5, 1.75, 2.5 and 2.75, so
    // rounding would have left the original bug intact at every scale above 1.25 — including
    // the 1.75 in live use. Flooring inverts the error: a legacy client is told 1, draws 1x,
    // and is composited slightly soft. Soft is nearly free through an H.264 stream; clipped
    // chrome is broken. Only clients that cannot speak fractional scale see this at all.
    let advertised_integer = (scale.floor() as i32).max(1);
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
    (output, global, damage_tracker)
}

/// Insert the render timer for `fps` and remember its token.
///
/// Shared with `reconfigure_session`, which has to replace it: the tick interval is baked into
/// the closure, so a frame-rate change is a new timer rather than a new number.
fn install_render_timer(state: &mut Wado, fps: u32) -> crate::Result<()> {
    // ── Render timer ──────────────────────────────────────────────────────────
    let frame_nanos = 1_000_000_000 / fps.max(1) as u64;
    let frame_period = Duration::from_nanos(frame_nanos);
    let mut pacing = TickStats::new(fps);
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
    Ok(())
}


/// Change the shape of a **running** session — resolution, aspect ratio, frame rate, bitrate —
/// without stopping it.
///
/// **Why this exists as its own verb.** Until now the only way to change any of these was
/// `stop_session` + `start_session`, and `stop_session` kills every application the session has
/// launched. So "change the bitrate" meant "lose your browser", which is why nobody could do it
/// on the fly. The three things that actually have to change are the encoder, the capture
/// target and the `Output`; the desktop — the display, the space, the seat, the windows, the
/// processes — has nothing to do with any of them and is left alone.
///
/// What is rebuilt, and why each one:
///
/// | | |
/// |---|---|
/// | encoder + capture | both are allocated at a fixed resolution; a new size needs new ones |
/// | `Output` | **invariant #8** — Wayland cannot un-advertise a mode, so a resize is a fresh output, never a mutated one |
/// | damage tracker | it is built *from* an output and tracks that output's geometry |
/// | render timer | the tick interval is baked into the timer's closure, so a new fps is a new timer |
///
/// What is deliberately **not** rebuilt: the `GlesRenderer` and the dmabuf global. The global's
/// format list comes from the renderer, and the renderer does not care what size we draw — so
/// tearing them down would hand `failed()` to every client holding a dmabuf, for nothing.
pub fn reconfigure_session(
    state: &mut Wado,
    ec: &EncoderConfig,
    scale: f32,
) -> crate::Result<EncoderReport> {
    if !state.session_active {
        return Err(CompositorError::Other("no active session to reconfigure".into()));
    }
    let before = state.encoder_config.clone();
    let started = Instant::now();

    // Stop the clock first. A tick landing halfway through this would render against a damage
    // tracker belonging to an output that no longer exists.
    if let Some(token) = state.render_timer_token.take() {
        state.loop_handle.remove(token);
    }
    // Released before the replacements are built rather than after: both hold GPU memory sized
    // for the old resolution, and holding two sets alive at once buys nothing.
    state.encoder = None;
    state.capture = None;

    let buf_size: Size<i32, Buffer> = (ec.width as i32, ec.height as i32).into();
    let renderer = state
        .renderer
        .as_mut()
        .ok_or_else(|| CompositorError::Other("no renderer — session is not really up".into()))?;
    // Disjoint field borrows: `renderer` and `gbm` are different fields of `state`.
    let (encoder, capture, mut report, tier) = build_pipeline(renderer, &state.gbm, ec, buf_size)?;

    // Only rebuild the output if its **shape** changed. A bitrate change does not touch the
    // output at all, and rebuilding one costs a client its `wl_output` — see
    // `retire_output_global` for what that costs.
    let shape_changed = match &before {
        Some(b) => {
            b.width != ec.width
                || b.height != ec.height
                || b.fps != ec.fps
                || (state.output_scale - clamp_scale(scale)).abs() > f32::EPSILON
        }
        None => true,
    };
    if shape_changed {
        // New output before the old one goes, so no surface is ever on zero outputs.
        let (output, global, damage_tracker) = build_output(state, ec, scale);
        state.output_scale = clamp_scale(scale);
        if let Some(old) = state.output.take() {
            state.space.unmap_output(&old);
        }
        if let Some(old) = state.output_global.take() {
            retire_output_global(state, old);
        }
        state.output = Some(output);
        state.output_global = Some(global);
        state.damage_tracker = Some(damage_tracker);
    }
    state.capture = Some(capture);
    state.encoder = Some(encoder);
    state.current_tier = Some(tier);
    state.encoder_config = Some(ec.clone());

    let moved = if shape_changed { refit_windows(state) } else { 0 };

    install_render_timer(state, ec.fps)?;

    // The stream's SPS changes here, so a decoder that is mid-GOP has nothing it can use until
    // the next IDR. Without this the viewer sees the old size frozen, or garbage, for up to a
    // keyframe interval.
    force_keyframe(state);
    // The previous shape's congestion history is not about this one — a divisor earned at 1080p
    // must not throttle a session that was just asked to run at 720p.
    state.congestion.reset();
    state.viewer_strained = false;
    let _ = state.shedding_tx.send(1);

    report.bitrate_kbps = ec.bitrate_kbps;
    report.fps = ec.fps;
    state.encoder_report = Some(report.clone());

    info!(
        from = before.as_ref().map(|b| format!("{}x{}@{} {}kbps", b.width, b.height, b.fps, b.bitrate_kbps)),
        to = %format!("{}x{}@{} {}kbps", ec.width, ec.height, ec.fps, ec.bitrate_kbps),
        scale,
        shape_changed,
        windows = state.space.elements().count(),
        moved,
        took_ms = started.elapsed().as_millis() as u64,
        pipeline = %report.pipeline,
        "session reconfigured — applications kept"
    );
    Ok(report)
}

/// How long a replaced `wl_output` global stays alive after it stops being advertised.
///
/// Generous, because the cost of being wrong is a dead application and the cost of being
/// patient is one unused global.
const GLOBAL_RETIRE: Duration = Duration::from_secs(5);

/// Stop advertising an output global now; destroy it later.
///
/// **The bug this closes, measured 2026-09-13 02:55:49.** A reconfigure removed the old output
/// global immediately and the session's application — a kitty holding a dmabuf — was gone
/// within 500 ms. `session reconfigured ... windows=1` and, on the very next line of the same
/// session, `windows=0`.
///
/// Removing a global is not a polite request. A client that still holds a bound `wl_output`
/// finds out by sending a request to an object that no longer exists, which is a protocol error,
/// which disconnects it — and a Wayland client whose display dies exits. The `global_remove`
/// event is how it is *supposed* to learn, and it needs a round trip to act on it.
///
/// So: `disable_global` stops new binds and sends `global_remove` immediately, and the object
/// itself lives on until a timer fires. Clients get their round trip.
fn retire_output_global(state: &mut Wado, id: smithay::reexports::wayland_server::backend::GlobalId) {
    state.display_handle.disable_global::<Wado>(id.clone());
    let res = state.loop_handle.insert_source(
        Timer::from_duration(GLOBAL_RETIRE),
        move |_, _, state: &mut Wado| {
            state.display_handle.remove_global::<Wado>(id.clone());
            tracing::debug!("retired a replaced wl_output global");
            TimeoutAction::Drop
        },
    );
    if let Err(e) = res {
        // Not fatal: the global stays disabled, which is the half that matters. It leaks one
        // object until the process exits, and that is strictly better than killing a client.
        warn!("could not schedule the old output global for removal, leaving it disabled: {e}");
    }
}

/// Put the windows back inside the output after its geometry changed.
///
/// Two cases, and only two on purpose:
///
/// * **Maximized** windows are sized *to* the output, so they are told the new size. An app that
///   is not reconfigured keeps drawing at the old one and is either clipped or letterboxed.
/// * **Everything else** is clamped so its top-left stays on the output. A window whose corner
///   is off the new screen cannot be dragged back — there is nothing left to grab.
///
/// ponytail: does not rescale or re-tile ordinary windows. Rotating a phone from landscape to
/// portrait will leave a wide window wide. Re-tiling is a policy decision and there is no tiling
/// policy here yet; clamping is the part that is unambiguously a bug if it is missing.
fn refit_windows(state: &mut Wado) -> usize {
    let Some(output) = state.output.clone() else {
        return 0;
    };
    let Some(geo) = state.space.output_geometry(&output) else {
        return 0;
    };

    let maximized: Vec<_> = state
        .space
        .elements()
        .filter(|w| {
            w.toplevel().is_some_and(|t| {
                t.with_pending_state(|s| s.states.contains(xdg_toplevel::State::Maximized))
            })
        })
        .cloned()
        .collect();
    for window in maximized {
        if let Some(t) = window.toplevel() {
            t.with_pending_state(|s| s.size = Some(geo.size));
            t.send_pending_configure();
        }
        state.space.map_element(window, (0, 0), false);
    }

    let strays: Vec<(smithay::desktop::Window, smithay::utils::Point<i32, smithay::utils::Logical>)> = state
        .space
        .elements()
        .filter_map(|w| {
            let loc = state.space.element_location(w)?;
            let x = loc.x.clamp(0, (geo.size.w - 1).max(0));
            let y = loc.y.clamp(0, (geo.size.h - 1).max(0));
            (x != loc.x || y != loc.y).then(|| (w.clone(), (x, y).into()))
        })
        .collect();
    let moved = strays.len();
    for (window, loc) in strays {
        state.space.map_element(window, loc, false);
    }
    moved
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
    // Kept apart on purpose: `asked` is what the client will compare against when it asks
    // which applications are running, `command` is what actually runs.
    let asked = command.trim().to_string();
    let command = &with_ime_flag(command);

    // Through a shell, in its own process group, and in the session's own environment —
    // see `proc::spawn` and `crate::session_env`.
    //
    // This grants no access that did not exist: the field was already free-form and spawned
    // whatever it named. It is still the reason the direct-mode control plane binds
    // localhost, and the reason the relay's Remote ID is the only thing between a stranger
    // and this shell — see the auth gate in TODO's NECESSARY list.
    let spawned = crate::proc::spawn(command, &state.app_env);
    match spawned {
        Ok(child) => {
            info!(pid = child.id(), command, "launched session application");
            state.app_processes.push(crate::proc::Launched {
                command: asked,
                child,
            });
        }
        // Not fatal to the session: the stream still runs, the window is just empty.
        Err(e) => tracing::error!("failed to launch session app {command:?}: {e}"),
    }
}

/// Which launched commands are still running, reaping the ones that are not.
///
/// **"Running" means the process is alive, not that it has a window.** Those differ for a few
/// seconds at startup, and for good in the case of an application that exits without ever
/// mapping one. Tying it to a window would mean matching `xdg_toplevel.app_id` against a
/// desktop entry's `Exec`, and those two strings disagree constantly (`org.gnome.Nautilus`
/// versus `nautilus`) — a dot that is briefly early is worth more than one that is often wrong.
///
/// ponytail: reaped here, on the question, rather than by a timer. Nothing else needs to know
/// an application has exited, so nothing else has to be woken to find out.
pub fn running_apps(state: &mut Wado) -> Vec<String> {
    state
        .app_processes
        .retain_mut(|app| !matches!(app.child.try_wait(), Ok(Some(_))));
    state
        .app_processes
        .iter()
        .map(|app| app.command.clone())
        .collect()
}

/// Add `--enable-wayland-ime` to a Chromium-family command that does not already have it.
///
/// **Why this is not the app's business.** Chromium binds `zwp_text_input_manager_v3` whether or
/// not the flag is present — observed here, repeatedly — but binding is not using: without the
/// flag it never constructs the Wayland input-method context, so it never calls `enable` on a text
/// field and the compositor never learns that one is focused. The phone keyboard then only opens
/// from the ⌨ button. A `.desktop` file written for a laptop has no reason to carry the flag, and
/// the user did not type the command, so this is the only place that can add it.
///
/// A previous run withdrew the claim that this flag was needed, on the grounds that the protocol
/// was bound without it. That reasoning was wrong: what was observed was the bind, and the thing
/// that is missing is the `enable`.
///
/// Substring matching on the program name only, deliberately. It is a launcher hint, not a
/// security boundary — the command was already free-form.
fn with_ime_flag(command: &str) -> String {
    const FLAG: &str = "--enable-wayland-ime";
    if command.contains(FLAG) {
        return command.to_string();
    }
    let program = command.split_whitespace().next().unwrap_or_default();
    let program = program.rsplit('/').next().unwrap_or(program);
    if !["chromium", "chrome", "google-chrome", "brave", "vivaldi", "microsoft-edge"]
        .iter()
        .any(|p| program.contains(p))
    {
        return command.to_string();
    }
    info!(program, "adding {FLAG} so the app can ask for text input");
    format!("{command} {FLAG}")
}

/// Tear down the active session and free its resources. Idempotent.
pub fn stop_session(state: &mut Wado) {
    if !state.session_active {
        return;
    }

    // Counted before the apps are killed, or the verdict below reports whatever survived the
    // teardown race rather than what the session actually had.
    let mapped_at_stop = state.space.elements().count();

    if let Some(token) = state.render_timer_token.take() {
        state.loop_handle.remove(token);
    }
    // Whole groups, not single pids: `child.kill()` reaped the shell and left everything it
    // had forked running — a browser kept playing audio after the session it belonged to was
    // gone. See `proc::terminate`.
    for mut app in state.app_processes.drain(..) {
        crate::proc::terminate(&mut app.child);
    }
    // After the applications, never before: killing the bus first would take the socket out
    // from under processes that are still shutting down.
    if let Some(bus) = state.app_bus.take() {
        crate::session_env::bus::terminate(bus);
    }
    state.app_env = crate::session_env::AppEnv::Host;
    if let Some(output) = state.output.take() {
        state.space.unmap_output(&output);
    }
    if let Some(global) = state.output_global.take() {
        state.display_handle.remove_global::<Wado>(global);
    }
    // Goes with the renderer that produced its format list. A client holding a dmabuf across
    // a session boundary gets `failed()` from the import handler, which is the honest answer.
    if let Some(global) = state.dmabuf_global.take() {
        state
            .dmabuf_state
            .destroy_global::<Wado>(&state.display_handle, global);
    }
    state.renderer = None;
    state.capture = None;
    state.gbm = None;
    state.damage_tracker = None;
    state.encoder = None;
    state.current_tier = None;
    state.encoder_config = None;
    state.encoder_report = None;
    state.frame_sink = None;
    state.session_active = false;
    state.window_move = None;
    state.pending_placement.clear();
    state.cascade_count = 0;

    // The negative case said out loud. A verdict that only logs when the answer is yes is a
    // verdict that reads as "no" and as "nobody looked" in exactly the same way — which is how
    // this question went unanswered in the first place.
    if !state.dmabuf_logged {
        // `windows` is what makes this a finding rather than a tautology. A session that
        // nobody launched an app into produced no buffers of any kind, so "unused" there means
        // "nothing asked", not "everything chose shm" — and the first such line was read as
        // the latter within a minute of shipping it. Zero windows, vacuous verdict.
        info!(
            windows = mapped_at_stop,
            "dmabuf path unused this session — every client buffer went through wl_shm"
        );
    }
    state.dmabuf_logged = false;

    // The other half of the presentation verdict. Reported with `windows` for the same reason as
    // dmabuf: with no app in the session nothing could have asked, so the line is vacuous rather
    // than a finding.
    if !state.presentation_logged {
        info!(
            windows = mapped_at_stop,
            "presentation feedback never collected this session — no client asked when its frames were shown"
        );
    }
    state.presentation_logged = false;

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
/// A viewer attached or went away. **Never** a session teardown — see `state.viewer_attached`.
///
/// This is also the one place per-viewer state is reset, which used to be spread across
/// `start_session` (congestion, strain, shed divisor) and the relay client's rejoin handler
/// (strain, keyframe). Two places that had to stay in sync and did not: a rejoin skipped
/// `start_session`, so a new decoder inherited the divisor the *previous* viewer's phone had
/// asked for and was throttled from its first frame for a fault it never had.
pub fn set_viewer_attached(state: &mut Wado, attached: bool) {
    if state.viewer_attached == attached {
        return;
    }
    state.viewer_attached = attached;
    if attached {
        // **Not** a full reset — see `Congestion::reattach`. Resetting here meant every
        // reconnect re-flooded the viewer at the full frame rate, and on the mobile links this
        // branch exists for that is once every couple of minutes.
        state.congestion.reattach();
        state.viewer_strained = false;
        // Assume on screen until told otherwise — the client re-asserts visibility on attach,
        // and defaulting the other way would black out every viewer that never sends it.
        state.viewer_visible = true;
        let _ = state.shedding_tx.send(state.congestion.divisor());
        // It also has no reference frame, so without this it shows black until the next
        // periodic keyframe — up to two seconds on a reattach that otherwise worked.
        force_keyframe(state);
        info!("viewer attached — rendering resumed");
    } else {
        info!(
            windows = state.space.elements().count(),
            "viewer detached — rendering paused; the session and its applications are kept"
        );
    }
}

/// The viewer's page went off screen, or came back. **Never** a session teardown.
///
/// A hidden page still holds a live peer connection and still receives RTP — the browser just
/// stops pulling frames and throws them away. Measured 2026-09-13 12:34:57: 11.9 Mbps leaving
/// this process, 65 kbps reaching the decoder. Rendering for that spends the viewer's mobile
/// data and this machine's encoder on something nobody can see.
pub fn set_viewer_visible(state: &mut Wado, visible: bool) {
    if state.viewer_visible == visible {
        return;
    }
    state.viewer_visible = visible;
    if visible {
        // The decoder has been discarding frames the whole time it was hidden, so it holds no
        // reference it can build on.
        force_keyframe(state);
        info!("viewer's page is back on screen — rendering resumed");
    } else {
        info!(
            windows = state.space.elements().count(),
            "viewer's page went off screen — rendering paused; the session is kept"
        );
    }
}

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

    // Shed this tick if the pump has been refusing frames. Deliberately decided *here* —
    // before anything is captured, encoded or collected — because the point is to not do the
    // work, and because a skip taken later would have to unpick the presentation-feedback
    // bookkeeping below. See `crate::congestion` for why the signal is the sink's drop count.
    //
    // What still happens on a shed tick, and must: the frame callbacks at the end of this
    // function. A client that stops receiving them stops drawing entirely, which would turn a
    // reduced frame rate into the freeze this exists to avoid.
    let dropped_total = state.frame_sink.as_ref().map_or(0, |s| s.dropped());
    // Nobody watching: render nothing. Not a mitigation like shedding — there is simply no
    // consumer, and every frame made here would be encoded, handed to a pump with no peer
    // connection behind it, and dropped. Congestion is not consulted (and so does not advance
    // its window) because a detached stretch says nothing about how a link was coping.
    // Nothing is rendered if any part of the pipeline is missing.
    //
    // The three `unwrap()`s below used to be unconditional, so **any** path that left the
    // session marked active with a half-built pipeline became a panic — caught, but caught by
    // the guard whose recovery is `stop_session`, which kills every application the session
    // launched. A failed `reconfigure_session` is exactly such a path: it releases the encoder
    // and the capture target before building their replacements, so a `?` in between leaves
    // this state behind. Turning a recoverable encoder error into a destroyed desktop is a much
    // worse outcome than skipping frames until someone fixes the configuration.
    let pipeline_ready = state.renderer.is_some()
        && state.capture.is_some()
        && state.damage_tracker.is_some()
        && state.encoder.is_some();
    if !pipeline_ready && !state.pipeline_gap_logged {
        state.pipeline_gap_logged = true;
        warn!(
            "the render pipeline is incomplete — skipping frames but keeping the session and its \
             applications. A reconfigure that could not build an encoder is the usual cause."
        );
    }
    if pipeline_ready && state.pipeline_gap_logged {
        state.pipeline_gap_logged = false;
        info!("the render pipeline is whole again");
    }
    let render_this_tick = pipeline_ready
        && state.viewer_attached
        && state.viewer_visible
        && state.congestion.should_render(dropped_total, state.viewer_strained);
    // On change only — it is state the viewer latches, and the divisor changes at most once per
    // decision window anyway.
    state.shedding_tx.send_if_modified(|cur| {
        let now = state.congestion.divisor();
        if *cur == now { false } else { *cur = now; true }
    });

    // The block yields the encode result plus how long each stage took, so neither
    // duration needs a dummy initial value.
    let (result, capture_dur, encode_dur): (crate::Result<Option<Vec<u8>>>, Duration, Duration) = if !render_this_tick {
        (Ok(None), Duration::ZERO, Duration::ZERO)
    } else {
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

    // Presentation feedback is answered from this, so it has to say whether the composite
    // actually reached the pipeline — not whether the encoder emitted a NAL this tick (it
    // legitimately holds frames).
    let composited = result.is_ok();

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

    if let Some(timing) = state.timing.as_mut().filter(|_| render_this_tick) {
        // `queue` is measured by the pump (it is the only side that knows when the frame
        // was taken), so it is not passed here — see `StageTimings::queue_ms`.
        timing.frame(tick_start, capture_dur, encode_dur, Duration::ZERO);
    }

    // Post-frame bookkeeping (after the capture/encode borrows are released).
    //
    // Presentation feedback is collected and answered in this one block, deliberately: a
    // callback that is taken and never answered leaves the client waiting forever, the same
    // failure mode as an unanswered dmabuf `ImportNotifier`. Collecting here means there is no
    // path between the take and the answer that can return early. (`OutputPresentationFeedback`
    // also discards on drop, so even a panic between the two cannot strand a client.)
    //
    // `Refresh` comes from the output mode rather than a separate constant so the rate a client
    // is told matches the one `wl_output` advertises. The flags are empty on purpose: `Vsync`,
    // `HwClock` and `HwCompletion` each assert a property of a real scanout, and `ZeroCopy`
    // would claim the frame went to a display instead of an encoder. Empty is the honest
    // encoding of "software composite, timestamped as accurately as this loop can".
    //
    // The count is taken in the flags closure because `OutputPresentationFeedback` does not
    // expose how much it collected, and without it this protocol would be the one thing in the
    // log that only ever speaks when the answer is yes — the hole the dmabuf verdict already had.
    // A shed tick collects nothing. That is the correct answer rather than a convenient one:
    // the client's buffer genuinely has not been presented yet, so its callbacks stay pending in
    // the surface's cached state and are answered by the next tick that does composite. The
    // alternative — collecting and discarding them every shed tick — would be a stream of
    // `discarded` events for frames that are still perfectly on their way to being shown.
    let fb_surfaces = std::cell::Cell::new(0usize);
    let mut feedback = OutputPresentationFeedback::new(&output);
    for window in state.space.elements().filter(|_| render_this_tick) {
        window.take_presentation_feedback(
            &mut feedback,
            |_, _| Some(output.clone()),
            |_, states| {
                // Scoped so the cached-state guard is released before smithay takes it again
                // to drain the same callbacks.
                {
                    let mut guard = states.cached_state.get::<PresentationFeedbackCachedState>();
                    if !guard.current().callbacks.is_empty() {
                        fb_surfaces.set(fb_surfaces.get() + 1);
                    }
                }
                wp_presentation_feedback::Kind::empty()
            },
        );
    }
    if !state.presentation_logged && fb_surfaces.get() > 0 {
        state.presentation_logged = true;
        info!(
            surfaces = fb_surfaces.get(),
            seq = state.frame_seq,
            "presentation feedback answered — a client is pacing on our timestamps"
        );
    }
    if composited && render_this_tick {
        let refresh = output
            .current_mode()
            .map(|m| Refresh::fixed(Duration::from_secs_f64(1000.0 / m.refresh as f64)))
            .unwrap_or(Refresh::Unknown);
        feedback.presented(state.clock.now(), refresh, state.frame_seq, wp_presentation_feedback::Kind::empty());
    }
    // Not `+= 1` on the presented branch only: the sequence counts composites, and a client
    // seeing a gap is being told the truth about a frame it never got. A shed tick is not a
    // composite at all, so it does not advance it.
    if render_this_tick {
        state.frame_seq += 1;
    }

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

/// Bits of encoded video per pixel per frame: `kbps * 1000 / (width * height * fps)`.
///
/// Not a quality metric in itself — it is the budget. H.264 at a given preset needs roughly a
/// fixed number of bits per pixel to look clean on moving content, so this says whether a
/// resolution/frame-rate/bitrate combination has asked for the impossible before anyone watches
/// it. Zero if any term is zero rather than a division by zero.
fn bits_per_pixel(ec: &EncoderConfig) -> f64 {
    let pixels = ec.width as u64 * ec.height as u64 * ec.fps as u64;
    if pixels == 0 {
        return 0.0;
    }
    (ec.bitrate_kbps as f64 * 1000.0) / pixels as f64
}

#[cfg(test)]
mod tests {
    use super::bits_per_pixel;
    use crate::conf::EncoderConfig;

    #[test]
    fn bits_per_pixel_halves_when_fps_doubles() {
        let cfg = |fps| EncoderConfig {
            width: 1080,
            height: 2422,
            fps,
            bitrate_kbps: 5676,
            ..crate::conf::WadoConfig::default().encoder
        };
        let at60 = bits_per_pixel(&cfg(60));
        let at120 = bits_per_pixel(&cfg(120));
        assert!((at60 / at120 - 2.0).abs() < 1e-9, "{at60} vs {at120}");
        // The value that matters: this config at 120 is near the 0.016 starvation figure.
        assert!(at120 < 0.05 && at120 > 0.01, "unexpected magnitude: {at120}");
        // No panic and no NaN on a degenerate config.
        assert_eq!(bits_per_pixel(&cfg(0)), 0.0);
    }
}

#[cfg(test)]
mod ime_flag_tests {
    use super::with_ime_flag;

    #[test]
    fn chromium_family_gets_the_flag_once() {
        assert_eq!(with_ime_flag("chromium"), "chromium --enable-wayland-ime");
        assert_eq!(
            with_ime_flag("/usr/bin/google-chrome-stable --new-window"),
            "/usr/bin/google-chrome-stable --new-window --enable-wayland-ime"
        );
        // Already there, in any position — do not add a second copy.
        let already = "chromium --enable-wayland-ime --new-window";
        assert_eq!(with_ime_flag(already), already);
    }

    #[test]
    fn everything_else_is_left_alone() {
        for cmd in ["foot", "nautilus --new-window", "firefox", ""] {
            assert_eq!(with_ime_flag(cmd), cmd);
        }
    }

    /// The match is on the program, not the arguments — a path that merely mentions a browser
    /// must not turn an unrelated command into a Chromium launch.
    #[test]
    fn only_the_program_name_is_matched() {
        assert_eq!(with_ime_flag("foot -e ./chrome-notes.sh"), "foot -e ./chrome-notes.sh");
    }
}
