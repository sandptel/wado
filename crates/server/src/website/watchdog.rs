//! Viewer watchdog — one job: stop a direct-mode session whose viewer has vanished.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tracing::warn;
use wado_compositor::CompositorCommand;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use super::{CmdSender, now_ms};

/// How long a started session may go without a viewer before it is stopped. See the twin in
/// `relay_client.rs` for the full reasoning.
const VIEWER_GRACE: std::time::Duration = std::time::Duration::from_secs(45);

/// Stop a direct-mode session whose viewer has vanished.
///
/// Direct mode had only two stop triggers: an explicit `POST /session/stop`, and the WebRTC
/// peer reaching `Failed`/`Closed`. The second cannot fire at all in the window that matters:
/// `POST /session/start` brings up the compositor, encoder and render loop **before** any peer
/// connection exists, so a browser that dies, navigates away or loses the network before
/// `POST /offer` leaves `active_pc` as `None` with no callback registered — nothing in the
/// process can ever send `Stop`. The session then runs forever at full frame rate with no
/// viewer, and `control.rs` rejects every later start with "a session is already active", so
/// the daemon is unusable until restarted.
///
/// Two conditions, as in relay mode: silence alone would kill a connected-but-idle viewer, so
/// the session is stopped only when HTTP has been silent *and* WebRTC is not connected.
pub(super) async fn viewer_watchdog(
    cmd_tx: CmdSender,
    last_request: Arc<AtomicU64>,
    session_started: Arc<AtomicBool>,
    active_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tick.tick().await;
        if !session_started.load(Ordering::SeqCst) {
            continue;
        }
        let silent_ms = now_ms().saturating_sub(last_request.load(Ordering::Relaxed));
        if silent_ms < VIEWER_GRACE.as_millis() as u64 {
            continue;
        }
        let connected = {
            let pc = active_pc.lock().unwrap_or_else(|e| e.into_inner()).clone();
            pc.map(|pc| pc.connection_state() == RTCPeerConnectionState::Connected)
                .unwrap_or(false)
        };
        if connected {
            continue;
        }
        warn!(
            silent_ms,
            "no sign of a viewer for {VIEWER_GRACE:?} and WebRTC is not connected — stopping \
             the session so its applications do not outlive it"
        );
        session_started.store(false, Ordering::SeqCst);
        let _ = cmd_tx.send(CompositorCommand::Stop);
    }
}
