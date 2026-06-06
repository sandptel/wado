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
pub mod conf;
pub mod control;
pub mod encode;
pub mod error;
pub mod grabs;
pub mod handlers;
pub mod headless;
pub mod input;
pub mod sink;
pub mod state;

use std::panic::AssertUnwindSafe;

use smithay::reexports::{
    calloop::{
        EventLoop,
        channel::{Event as ChannelEvent, Sender, channel},
    },
    wayland_server::Display,
};
use tokio::sync::mpsc;

pub use control::CompositorCommand;
pub use error::{CompositorError, Result};
pub use sink::channel::FrameMsg;
pub use state::Wado;

/// The `Send`able sender for [`CompositorCommand`]s into the calloop loop. Re-exported
/// (as an alias over the calloop type) so the server can name and use it without
/// depending on smithay/calloop itself.
pub type CommandSender = Sender<CompositorCommand>;

/// Build the compositor: create the event loop, display, and [`Wado`] state, claim the
/// Wayland socket (exported via `WAYLAND_DISPLAY` for apps spawned into the session),
/// and insert the [`CompositorCommand`] source (panic-guarded). Returns the loop and
/// state for the caller to `run`, plus the `Sender` the server uses to drive sessions.
///
/// `frame_tx` is the encoded-frame channel; the session's `ChannelSink` clones it and
/// the server owns the matching receiver (the WebRTC frame pump).
pub fn build(
    frame_tx: mpsc::Sender<FrameMsg>,
) -> Result<(EventLoop<'static, Wado>, Wado, CommandSender)> {
    let mut event_loop: EventLoop<'static, Wado> =
        EventLoop::try_new().map_err(|e| CompositorError::Other(format!("event loop: {e}")))?;
    let display: Display<Wado> =
        Display::new().map_err(|e| CompositorError::Other(format!("display: {e}")))?;
    let state = Wado::new(&mut event_loop, display);

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

    Ok((event_loop, state, cmd_tx))
}
