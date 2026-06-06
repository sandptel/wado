//! Commands the server sends to the compositor, and the handler that runs them with
//! `&mut Wado` on the calloop thread.
//!
//! This is the compositor's half of the server↔compositor boundary: the server holds
//! the [`CompositorCommand`] `Sender` and never touches `Wado` or any Smithay type
//! directly. The command source (with its panic guard) is inserted in [`crate::build`].
//!
//! ## Future: input
//! Input (touch / pointer / keyboard from the remote client) will NOT be added as more
//! `CompositorCommand` variants. Per wado's "input never rides behind video" invariant
//! it gets its own, separate `calloop::channel` of `InputEvent`s — the server parses the
//! WebRTC data channel into `InputEvent`s and the compositor synthesizes Smithay
//! `PointerHandle`/`KeyboardHandle`/`TouchHandle` events on its `seat`. Keeping command
//! and input on independent channels is deliberate; do not multiplex them.

use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::{
    Wado,
    conf::SessionConfig,
    headless,
    sink::channel::{ChannelSink, FrameMsg},
};

/// A request from the server, marshalled onto the compositor thread.
pub enum CompositorCommand {
    /// Spin up a compositor session with the given config and launch its app.
    Start {
        config: SessionConfig,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Tear down the active session (idempotent).
    Stop,
    /// Launch a command into the *running* session (in realtime, any number of times).
    /// Ignored with a warning when no session is active.
    Launch { command: String },
    /// Make the next encoded frame a forced IDR keyframe. Sent when a viewer
    /// connects or the browser requests one via RTCP PLI/FIR.
    ForceKeyframe,
}

/// Run one command on the calloop thread. `frame_tx` is the pump sender, cloned
/// into the new session's [`ChannelSink`].
pub fn handle_command(state: &mut Wado, cmd: CompositorCommand, frame_tx: &mpsc::Sender<FrameMsg>) {
    match cmd {
        CompositorCommand::Start { config, reply } => {
            let _ = reply.send(start(state, &config, frame_tx));
        }
        CompositorCommand::Stop => headless::stop_session(state),
        CompositorCommand::Launch { command } => {
            if state.session_active {
                headless::launch_command(state, &command);
            } else {
                tracing::warn!("launch ignored — no active session");
            }
        }
        CompositorCommand::ForceKeyframe => headless::force_keyframe(state),
    }
}

fn start(
    state: &mut Wado,
    config: &SessionConfig,
    frame_tx: &mpsc::Sender<FrameMsg>,
) -> Result<(), String> {
    if state.session_active {
        return Err("a session is already active".into());
    }
    let encoder = crate::conf::to_encoder_config(config);
    let frame_dur = Duration::from_nanos(1_000_000_000 / encoder.fps.max(1) as u64);
    let sink = Box::new(ChannelSink::new(frame_tx.clone(), frame_dur));

    // start_session and launch_command emit their own tracing logs.
    headless::start_session(state, &encoder, sink).map_err(|e| e.to_string())?;
    // Optional initial command (empty → blank session; launch more at runtime).
    if !config.command.trim().is_empty() {
        headless::launch_command(state, &config.command);
    }
    Ok(())
}
