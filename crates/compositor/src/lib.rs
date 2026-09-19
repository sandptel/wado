//! `wado-compositor` — wado's headless Smithay compositor as a library.
//!
//! It owns all of the Wayland-protocol state, the GLES render pipeline, the x264
//! encoder, and the per-session lifecycle. The server crate (`wado`) drives it
//! **only** through the typed boundary exposed here — a [`CompositorCommand`] `Sender`
//! and a frame channel ([`sink::channel::FrameMsg`]) — and never references [`Wado`]
//! or any Smithay type directly.
//!
//! Use [`build`] to construct the event loop + state and obtain the command `Sender`;
//! the caller (the `wado` binary's `main`) runs the returned loop on its thread.
//!
//! ## Crash isolation (interim)
//! [`build`] guards the command source with `catch_unwind`, and the render timer in
//! [`headless`] is guarded too, so a compositor **panic** tears down only the active
//! session — the host process (and the server's WebRTC/HTTP/SSE connections) survive.
//! This covers Rust *unwinding* panics only; a segfault in native EGL/GLES/Mesa/x264
//! still aborts the process. Surviving that requires running the compositor as a
//! supervised child process — a deliberately deferred milestone. The build must stay
//! `panic = "unwind"` (the default) for the guards to work.

#![allow(irrefutable_let_patterns)]

pub mod capture;
pub mod congestion;
pub mod conf;
pub mod control;
pub mod encode;
pub mod error;
pub mod focus;
pub mod grabs;
pub mod handlers;
pub mod headless;
pub mod input;
pub mod pacing;
pub mod placement;
pub mod proc;
pub mod sink;
pub mod state;
pub mod timing;
mod window;

use std::panic::AssertUnwindSafe;

use smithay::reexports::{
    calloop::{
        EventLoop,
        channel::{Event as ChannelEvent, Sender, channel},
        signals::{Signal, Signals},
    },
    wayland_server::Display,
};
use tokio::sync::mpsc;

pub use control::CompositorCommand;
pub use error::{CompositorError, Result};
pub use sink::channel::FrameMsg;
pub use wado_protocol::StageTimings;
pub use state::Wado;
pub use wado_protocol::InputEvent;

/// The `Send`able sender for [`CompositorCommand`]s into the calloop loop. Re-exported
/// (as an alias over the calloop type) so the server can name and use it without
/// depending on smithay/calloop itself.
pub type CommandSender = Sender<CompositorCommand>;
/// The `Send`able sender for remote [`InputEvent`]s into the calloop loop. A **separate**
/// channel from commands (input never rides behind control or video — invariant #1).
pub type InputSender = Sender<InputEvent>;

/// Channels the server uses to drive the compositor, returned by [`build`]. The server
/// holds these and nothing else of the compositor — no Smithay type crosses the boundary.
pub struct CompositorHandles {
    /// Session lifecycle: Start / Stop / Launch / ForceKeyframe.
    pub commands: CommandSender,
    /// Remote touch + keyboard events (separate, low-latency channel).
    pub input: InputSender,
    /// Latest per-stage render timings (latest-value-wins; see [`timing`]).
    pub timings: tokio::sync::watch::Receiver<StageTimings>,
    /// Whether the focused application currently wants text input — `zwp_text_input_v3`,
    /// observed in `handlers/text_input.rs`. The server forwards changes to the viewer, which
    /// is what raises a phone's soft keyboard without anyone pressing a button.
    ///
    /// A `bool` over a watch channel, deliberately: it is state, not an event, so a viewer that
    /// connects mid-session gets the current answer rather than having missed the transition.
    pub text_input: tokio::sync::watch::Receiver<bool>,
    /// The render-tick divisor in force — see [`congestion`]. Forwarded to the viewer so it can
    /// tell a frame rate *it asked us to reduce* from a compositor that has stopped producing.
    pub shedding: tokio::sync::watch::Receiver<u32>,
}

/// Build the compositor: create the event loop, display, and [`Wado`] state, claim the
/// Wayland socket (exported via `WAYLAND_DISPLAY` for apps spawned into the session),
/// and insert the [`CompositorCommand`] source (panic-guarded). Returns the loop and
/// state for the caller to `run`, plus the `Sender` the server uses to drive sessions.
///
/// `frame_tx` is the encoded-frame channel; the session's `ChannelSink` clones it and
/// the server owns the matching receiver (the WebRTC frame pump).
pub fn build(
    frame_tx: mpsc::Sender<FrameMsg>,
) -> Result<(EventLoop<'static, Wado>, Wado, CompositorHandles)> {
    let mut event_loop: EventLoop<'static, Wado> =
        EventLoop::try_new().map_err(|e| CompositorError::Other(format!("event loop: {e}")))?;
    let display: Display<Wado> =
        Display::new().map_err(|e| CompositorError::Other(format!("display: {e}")))?;
    let mut state = Wado::new(&mut event_loop, display);

    // Per-stage timing publisher. Created here, not per session, so the server's receiver
    // survives stop/start cycles and never has to be re-plumbed.
    let (stage_timer, timings) = timing::StageTimer::new();
    state.timing = Some(stage_timer);

    // Apps spawned later (on session start) connect to this socket.
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    let (cmd_tx, cmd_channel) = channel::<CompositorCommand>();
    event_loop
        .handle()
        .insert_source(cmd_channel, move |event, _, state: &mut Wado| {
            if let ChannelEvent::Msg(cmd) = event {
                // A panic while handling a command must not abort the whole process
                // (which would take the server down with it). Catch it, log it, and
                // reset to idle so the loop keeps running and future Starts work.
                let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    control::handle_command(state, cmd, &frame_tx);
                }));
                if caught.is_err() {
                    tracing::error!("compositor panicked handling a command — resetting session");
                    headless::stop_session(state);
                }
            }
        })
        .map_err(|e| CompositorError::Other(format!("insert control source: {e}")))?;

    // Separate input channel (touch + keyboard). Same panic guard: a panic while
    // synthesizing input must not abort the process. We do NOT reset the session here —
    // a bad input event shouldn't kill a working stream; we just drop it and log.
    let (input_tx, input_channel) = channel::<InputEvent>();
    event_loop
        .handle()
        .insert_source(input_channel, move |event, _, state: &mut Wado| {
            if let ChannelEvent::Msg(ev) = event {
                let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    state.handle_remote_input(ev);
                }));
                if caught.is_err() {
                    tracing::error!("compositor panicked handling an input event — dropped");
                }
            }
        })
        .map_err(|e| CompositorError::Other(format!("insert input source: {e}")))?;

    // Shut down on a signal instead of dying on one. Without this, Ctrl-C or a `systemctl
    // stop` killed the process outright and `stop_session` never ran — so every application
    // launched into the session survived its own session, which is the other half of the
    // leak `proc.rs` describes. Handled here rather than in the server's `main` so the
    // calloop/Smithay types stay on this side of the crate boundary.
    //
    // A signalfd source, not a handler setting a flag: the loop is asleep in `poll` when the
    // signal lands, and a flag only gets read once something else happens to wake it.
    let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM])
        .map_err(|e| CompositorError::Other(format!("signal source: {e}")))?;
    event_loop
        .handle()
        .insert_source(signals, |event, _, state: &mut Wado| {
            tracing::info!(signal = ?event.signal(), "signal received — stopping session");
            // Before the loop stops, not after: `stop_session` is what kills the launched
            // applications, and it cannot run once the process is gone.
            headless::stop_session(state);
            state.loop_signal.stop();
        })
        .map_err(|e| CompositorError::Other(format!("insert signal source: {e}")))?;

    let text_input = state.text_input_tx.subscribe();
    let shedding = state.shedding_tx.subscribe();
    Ok((
        event_loop,
        state,
        CompositorHandles { commands: cmd_tx, input: input_tx, timings, text_input, shedding },
    ))
}
