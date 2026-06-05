//! Commands sent from the async control server (tokio) to the compositor
//! (calloop main thread), and the handler that runs them with `&mut Wado`.

use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::{
    Wado,
    conf::SessionConfig,
    headless,
    sink::channel::{ChannelSink, FrameMsg},
};

/// A request from the web client / WebRTC layer, marshalled onto the compositor thread.
pub enum ControlCommand {
    /// Spin up a compositor session with the given config and launch its app.
    Start {
        config: SessionConfig,
        reply: oneshot::Sender<Result<(), String>>,
    },
    /// Tear down the active session (idempotent).
    Stop,
    /// Make the next encoded frame a forced IDR keyframe. Sent when a viewer
    /// connects or the browser requests one via RTCP PLI/FIR.
    ForceKeyframe,
}

/// Run one command on the calloop thread. `frame_tx` is the pump sender, cloned
/// into the new session's [`ChannelSink`].
pub fn handle_command(
    state: &mut Wado,
    cmd: ControlCommand,
    frame_tx: &mpsc::Sender<FrameMsg>,
) {
    match cmd {
        ControlCommand::Start { config, reply } => {
            let _ = reply.send(start(state, &config, frame_tx));
        }
        ControlCommand::Stop => headless::stop_session(state),
        ControlCommand::ForceKeyframe => headless::force_keyframe(state),
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
    let encoder = config.to_encoder_config();
    let frame_dur = Duration::from_nanos(1_000_000_000 / encoder.fps.max(1) as u64);
    let sink = Box::new(ChannelSink::new(frame_tx.clone(), frame_dur));

    // start_session and spawn_session_command emit their own tracing logs.
    headless::start_session(state, &encoder, sink).map_err(|e| e.to_string())?;
    headless::spawn_session_command(state, &config.command);
    Ok(())
}
