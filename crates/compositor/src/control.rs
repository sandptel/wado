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
use wado_protocol::{SessionInfo, WindowAction};

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
        reply: oneshot::Sender<Result<SessionInfo, String>>,
    },
    /// Tear down the active session (idempotent).
    Stop,
    /// Change the shape of the running session in place — see
    /// [`crate::headless::reconfigure_session`]. The applications survive.
    Reconfigure {
        config: SessionConfig,
        reply: oneshot::Sender<Result<SessionInfo, String>>,
    },
    /// What is running right now, if anything.
    ///
    /// Exists so a client that did not start the session can still be told what it is. Answered
    /// from the compositor thread rather than a flag on the network side, because that flag is
    /// per-connection and a reconnecting viewer gets a fresh one — only the compositor knows
    /// whether a session survived.
    Status {
        reply: oneshot::Sender<Option<SessionInfo>>,
    },
    /// Launch a command into the *running* session (in realtime, any number of times).
    /// Ignored with a warning when no session is active.
    Launch { command: String },
    /// Make the next encoded frame a forced IDR keyframe. Sent when a viewer
    /// connects or the browser requests one via RTCP PLI/FIR.
    ForceKeyframe,
    /// Act on the focused window. See [`crate::window`].
    Window(WindowAction),
    /// The viewer's decoder is (or is no longer) saturated.
    ///
    /// A **level**, not an event: the client re-sends it only when its settled verdict changes,
    /// so between messages the last value stands. The render loop reads it once per decision
    /// window — see `crate::congestion`, which explains why it is counted rather than acted on.
    ///
    /// This is the only congestion signal wado cannot measure for itself. The server can see the
    /// pump back up; it cannot see a phone decoding 15 of the 90 frames a second it is being
    /// sent, which is a failure that has been measured here with every server-side metric clean.
    ViewerStrain(bool),
    /// A viewer's media path came up, or went away. **Not** a session lifecycle verb.
    ///
    /// The session outlives its viewers by design; this only says whether there is currently
    /// anyone to render for. `false` pauses the render tick and keeps everything else — the
    /// windows, the applications, the Wayland clients' frame callbacks. `true` resumes and
    /// resets the per-viewer state, because none of the previous viewer's history is about
    /// this one.
    ///
    /// The direct HTTP transport never sends it, and so keeps its old always-rendering
    /// behaviour rather than depending on a message it does not know to send.
    ViewerAttached(bool),
}

/// Run one command on the calloop thread. `frame_tx` is the pump sender, cloned
/// into the new session's [`ChannelSink`].
pub fn handle_command(state: &mut Wado, cmd: CompositorCommand, frame_tx: &mpsc::Sender<FrameMsg>) {
    match cmd {
        CompositorCommand::Start { config, reply } => {
            let _ = reply.send(start(state, &config, frame_tx));
        }
        CompositorCommand::Stop => headless::stop_session(state),
        CompositorCommand::Reconfigure { config, reply } => {
            let _ = reply.send(reconfigure(state, &config));
        }
        CompositorCommand::Status { reply } => {
            let info = state
                .session_active
                .then(|| state.encoder_report.clone())
                .flatten()
                .map(|encoder| SessionInfo { encoder });
            let _ = reply.send(info);
        }
        CompositorCommand::Launch { command } => {
            if state.session_active {
                headless::launch_command(state, &command);
            } else {
                tracing::warn!("launch ignored — no active session");
            }
        }
        CompositorCommand::ForceKeyframe => headless::force_keyframe(state),
        CompositorCommand::ViewerAttached(attached) => {
            headless::set_viewer_attached(state, attached)
        }
        CompositorCommand::ViewerStrain(strained) => {
            if state.viewer_strained != strained {
                tracing::info!(strained, "viewer reported a change in decoder strain");
            }
            state.viewer_strained = strained;
        }
        CompositorCommand::Window(action) => {
            if state.session_active {
                state.window_action(action);
            } else {
                tracing::warn!(?action, "window action ignored — no active session");
            }
        }
    }
}

/// Resolve a [`SessionConfig`] the same way `start` does and apply it to the running session.
///
/// Sharing `to_encoder_config` is the point: a bitrate the user asked for and a bitrate a
/// reconfigure applies have to come out of the same function, or `Quality::Balanced` means one
/// thing at start and another on a change.
fn reconfigure(state: &mut Wado, config: &SessionConfig) -> Result<SessionInfo, String> {
    let encoder = crate::conf::to_encoder_config(config);
    let report = headless::reconfigure_session(state, &encoder, config.scale)
        .map_err(|e| e.to_string())?;
    // The behaviour settings are re-applied too: they are part of "the session as configured",
    // and a reconfigure that silently kept the old keyboard repeat rate would be a surprise.
    if let Some(keyboard) = state.seat.get_keyboard() {
        keyboard.change_repeat_info(config.input.repeat_rate, config.input.repeat_delay);
    }
    state.placement = config.window.placement;
    state.focus_follows_pointer = config.input.focus_follows_pointer;
    Ok(SessionInfo { encoder: report })
}

fn start(
    state: &mut Wado,
    config: &SessionConfig,
    frame_tx: &mpsc::Sender<FrameMsg>,
) -> Result<SessionInfo, String> {
    if state.session_active {
        return Err("a session is already active".into());
    }
    let encoder = crate::conf::to_encoder_config(config);
    let frame_dur = Duration::from_nanos(1_000_000_000 / encoder.fps.max(1) as u64);
    let sink = Box::new(ChannelSink::new(frame_tx.clone(), frame_dur));

    // Sessions always start blank; apps are spawned at runtime via CompositorCommand::Launch.
    // start_session emits its own tracing logs and reports the encoder it actually opened.
    let encoder_report =
        headless::start_session(state, &encoder, config.scale, sink).map_err(|e| e.to_string())?;

    // Apply the per-domain behaviour settings (atomic sub-structs of SessionConfig).
    if let Some(keyboard) = state.seat.get_keyboard() {
        keyboard.change_repeat_info(config.input.repeat_rate, config.input.repeat_delay);
    }
    state.placement = config.window.placement;
    state.focus_follows_pointer = config.input.focus_follows_pointer;
    state.encoder_report = Some(encoder_report.clone());
    Ok(SessionInfo { encoder: encoder_report })
}
