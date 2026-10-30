//! A joined device's own room (spec §D2, R2): a seatless relay room (`control=1&strict=1`,
//! relay caps `control` + `room_strict`) whose first sealed message was `room_hello{view}`. It
//! never takes a seat, so joining a session displaces nobody at the relay; what it may do here is
//! this file's policy, never the relay's:
//!
//! - join the session, take it over, leave it, end it — each with its own grant (`join`,
//!   `takeover`; ending is anyone's, the user's decision), after its speed test;
//! - stream it and drive it, over its own peer connection, input tagged with its origin;
//! - the session's own verbs (launch, window actions, timings, the menu sheet) and the computer's
//!   controls under the same policy as a widget's link (`crate::control::policy`).
//!
//! Shells, files, the trackpad and the host clipboard history stay on the device's own link.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use wado_compositor::CompositorCommand;
use wado_protocol::relay::RelayMsg;
use wado_protocol::{SessionConfig, SessionInfo};

use super::{Hub, pc};
use crate::control::serve::{Ctx, Who};
use crate::linktest::verdict::{Ask, Intent, judge};
use crate::session::membership::Origin;

/// A new session's bitrate before it has one, for the speed test's arithmetic, kbps.
const NEW_SESSION_KBPS: u32 = 6000;

/// One joined room's own state on this daemon.
pub struct ViewRoom {
    pub origin: Origin,
    pub slot: pc::Slot,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl ViewRoom {
    pub fn new(origin: Origin) -> Self {
        Self {
            origin,
            slot: pc::Slot::default(),
            tasks: Mutex::default(),
        }
    }

    fn start_feeds(&self, hub: &Arc<Hub>, out: &mpsc::Sender<String>) {
        let mut t = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if t.is_empty() {
            *t = super::forward::spawn(hub, self.origin, out.clone());
        }
    }

    fn stop_feeds(&self) {
        for t in self
            .tasks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
            t.abort();
        }
    }

    /// The room is gone: out of the session, connection closed, feeds stopped.
    pub fn close(&self, hub: &Hub) {
        self.stop_feeds();
        hub.detach(self.origin);
        self.slot.close();
    }
}

async fn send(out: &mpsc::Sender<String>, msg: &RelayMsg) {
    if let Ok(t) = serde_json::to_string(msg) {
        let _ = out.send(t).await;
    }
}

/// The session the compositor is running, if any.
pub async fn live_session(cmd_tx: &wado_compositor::CommandSender) -> Option<SessionInfo> {
    let (tx, rx) = oneshot::channel();
    cmd_tx.send(CompositorCommand::Status { reply: tx }).ok()?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .ok()?
        .ok()
        .flatten()
}

/// Rebuild the running session at `config` (a fresh `Output` when the shape changes — invariant
/// 8), bounded like every other compositor round trip.
pub async fn reconfigure(
    cmd_tx: &wado_compositor::CommandSender,
    config: SessionConfig,
) -> Result<SessionInfo, String> {
    let (reply, rx) = oneshot::channel();
    cmd_tx
        .send(CompositorCommand::Reconfigure { config, reply })
        .map_err(|_| "compositor unavailable".to_string())?;
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => Err("compositor dropped reply".into()),
        Err(_) => Err("reconfigure timed out".into()),
    }
}

/// A device's speed test, judged and kept (the seat's and a room's alike).
pub fn verdict(
    hub: &Hub,
    key: &str,
    down_kbps: u32,
    rtt_ms: u32,
    intent: &str,
) -> wado_protocol::multi::LinkVerdict {
    let intent = Intent::parse(intent);
    if down_kbps > 0 {
        crate::linktest::headroom::measured(down_kbps);
    }
    let (_, participants, _, kbps) = hub.summary();
    let running = hub
        .session_started
        .load(std::sync::atomic::Ordering::SeqCst);
    let ask = Ask {
        intent,
        down_kbps,
        rtt_ms,
        session_kbps: match intent {
            Intent::Join if kbps > 0 => kbps,
            _ => NEW_SESSION_KBPS,
        },
        participants: match intent {
            Intent::Join if running => participants.len() as u32,
            _ => 0,
        },
        cap: wado_config::live::current().server.max_participants,
        host: crate::linktest::headroom::read(&hub.encoder()),
    };
    let v = judge(&ask);
    info!(
        intent = ?intent,
        down_kbps,
        rtt_ms,
        level = %v.level,
        max = v.max_participants,
        "speed test"
    );
    hub.record_verdict(key, intent, &v);
    v
}

/// One opened message on a joined room.
pub async fn handle(hub: &Arc<Hub>, room: &ViewRoom, who: &Who, text: &str, cx: &Ctx) {
    let msg = match serde_json::from_str::<RelayMsg>(text) {
        Ok(m) => m,
        Err(e) => {
            warn!(device = %who.name, "joined room: unreadable message — {e}");
            return;
        }
    };
    let out = &cx.out;
    let origin = room.origin;
    let member = || hub.is_member(origin);
    match msg {
        RelayMsg::RoomHello { .. } | RelayMsg::Pong => {}
        RelayMsg::Ping => send(out, &RelayMsg::Pong).await,
        RelayMsg::ClientLog { line } => info!("browser ({}): {line}", who.name),
        RelayMsg::SessionsRequest => {
            let sessions = crate::sessions::list();
            send(out, &RelayMsg::Sessions { sessions }).await;
        }
        RelayMsg::LinkTest { bytes } => crate::linktest::send(out.clone(), bytes),
        RelayMsg::LinkReport {
            down_kbps,
            rtt_ms,
            intent,
        } => {
            let verdict = verdict(hub, &who.key, down_kbps, rtt_ms, &intent);
            send(out, &RelayMsg::LinkVerdict { verdict }).await;
        }

        RelayMsg::SessionJoin | RelayMsg::SessionRejoin => {
            let Some(info) = live_session(&hub.cmd_tx).await else {
                let message = "that session has ended".to_string();
                return send(out, &RelayMsg::SessionError { message }).await;
            };
            match hub.join(origin, true) {
                Ok(()) => {
                    hub.detached
                        .store(false, std::sync::atomic::Ordering::SeqCst);
                    room.start_feeds(hub, out);
                    send(out, &RelayMsg::SessionStarted { info }).await;
                }
                Err(message) => {
                    info!(device = %who.name, "joined room: join refused — {message}");
                    send(out, &RelayMsg::SessionError { message }).await;
                }
            }
        }

        RelayMsg::SessionTakeover { config } => {
            if let Err(why) = config.validate() {
                let message = format!("that configuration cannot work: {why}");
                return send(out, &RelayMsg::SessionError { message }).await;
            }
            if live_session(&hub.cmd_tx).await.is_none() {
                let message = "that session has ended".to_string();
                return send(out, &RelayMsg::SessionError { message }).await;
            }
            let cfg = match hub.takeover(origin, &config) {
                Ok(c) => c,
                Err(message) => {
                    info!(device = %who.name, "joined room: take over refused — {message}");
                    return send(out, &RelayMsg::SessionError { message }).await;
                }
            };
            hub.detached
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = hub.audio_low.send(cfg.low_latency_audio);
            room.start_feeds(hub, out);
            // At the taker's size: a fresh output, the apps refitted.
            match reconfigure(&hub.cmd_tx, cfg).await {
                Ok(info) => {
                    hub.note_session(&info, true);
                    send(out, &RelayMsg::SessionStarted { info }).await;
                }
                Err(message) => send(out, &RelayMsg::SessionError { message }).await,
            }
        }

        RelayMsg::SdpOffer { sdp } => {
            if !member() {
                let message = "join the session before streaming it".to_string();
                return send(out, &RelayMsg::SessionError { message }).await;
            }
            let peer = format!("{} (joined)", who.name);
            match pc::answer(hub, origin, &sdp, &room.slot, peer).await {
                Ok(sdp) => send(out, &RelayMsg::SdpAnswer { sdp }).await,
                Err(e) => {
                    let message = format!("WebRTC setup failed: {e}");
                    send(out, &RelayMsg::SessionError { message }).await;
                }
            }
        }
        RelayMsg::IceCandidate { candidate } => {
            if let (Some(pc), Ok(c)) = (room.slot.current(), serde_json::from_str(&candidate)) {
                let _ = pc.add_ice_candidate(c).await;
            }
        }

        RelayMsg::SessionDetach => {
            room.stop_feeds();
            let empty = hub.leave(origin);
            room.slot.close();
            if empty {
                hub.detached
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
            info!(device = %who.name, "joined room: left the session; it goes on");
        }
        // Anyone in it may end it for everyone (the user's decision).
        RelayMsg::SessionStop => {
            // Anyone in it; from outside it, only a device that could take it over anyway.
            if !member() && !hub.gate.grants(&who.key).takeover {
                let message = "only someone in this session can end it — or a device with the \
                               takeover grant"
                    .to_string();
                return send(out, &RelayMsg::SessionError { message }).await;
            }
            info!(device = %who.name, "joined room: ended the session for everyone");
            hub.end(Some(origin));
            hub.session_started
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let _ = hub.cmd_tx.send(CompositorCommand::Stop);
            room.stop_feeds();
            room.slot.close();
            send(out, &RelayMsg::SessionStopped).await;
        }

        RelayMsg::ViewerVisible { visible } if member() => hub.visible(origin, visible),
        RelayMsg::ViewerStrain { strained } if member() => hub.strain(origin, strained),
        RelayMsg::SessionReconfigure { config } if member() => {
            if let Err(why) = config.validate() {
                let message = format!("that configuration cannot work: {why}");
                return send(out, &RelayMsg::SessionError { message }).await;
            }
            let result = match hub.reconfigure(origin, &config) {
                Some(c) => reconfigure(&hub.cmd_tx, c).await,
                None => live_session(&hub.cmd_tx)
                    .await
                    .ok_or_else(|| "that session has ended".to_string()),
            };
            match result {
                Ok(info) => send(out, &RelayMsg::SessionReconfigured { info }).await,
                Err(message) => send(out, &RelayMsg::SessionError { message }).await,
            }
        }
        RelayMsg::SessionLaunch { command } if member() => {
            let _ = hub.cmd_tx.send(CompositorCommand::Launch { command });
            send(out, &RelayMsg::SessionLaunched).await;
        }
        RelayMsg::SessionWindow { action } if member() => {
            let _ = hub.cmd_tx.send(CompositorCommand::Window(action));
            send(out, &RelayMsg::SessionWindowed).await;
        }
        RelayMsg::ClipboardSet { text } if member() => {
            let clip = wado_compositor::clipboard::Clip {
                mime: "text/plain;charset=utf-8".into(),
                data: text.into_bytes().into(),
                secret: false,
            };
            let _ = hub.cmd_tx.send(CompositorCommand::SetClipboard(clip));
        }
        RelayMsg::TimingRequest => {
            let mut timings = *hub.feeds.timings.borrow();
            timings.queue_ms = hub
                .feeds
                .queue_us
                .load(std::sync::atomic::Ordering::Relaxed) as f64
                / 1e3;
            send(out, &RelayMsg::Timing { timings }).await;
        }
        RelayMsg::AppsRequest => {
            let mut apps = crate::apps::discover();
            crate::apps::running::mark(&mut apps, &hub.cmd_tx).await;
            send(out, &RelayMsg::AppsList { apps }).await;
        }
        RelayMsg::TargetsRequest { seq, x, y, r } if member() => {
            let (cmd_tx, a11y, out) =
                (hub.cmd_tx.clone(), Arc::clone(&hub.feeds.a11y), out.clone());
            tokio::spawn(async move {
                let targets = crate::a11y::tap_targets(&cmd_tx, &a11y, x, y, r).await;
                send(&out, &RelayMsg::Targets { seq, targets }).await;
            });
        }
        RelayMsg::MenuActivate { id } if member() => {
            let (menu, a11y, out) = (
                hub.feeds.menu.clone(),
                Arc::clone(&hub.feeds.a11y),
                out.clone(),
            );
            tokio::spawn(async move {
                let spot = menu.borrow().clone();
                let Some(spot) = spot else { return };
                crate::menu_sheet::activate(&a11y, &spot, &id).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
                let spot = menu.borrow().clone();
                if let Some(spot) = spot {
                    let sheet = crate::menu_sheet::read(&a11y, &spot).await;
                    send(&out, &RelayMsg::Menu { menu: Some(sheet) }).await;
                }
            });
        }
        RelayMsg::DeviceStatus { link } => {
            hub.set_link(origin, crate::links::describe(&link));
        }
        // The computer's controls: exactly a widget's link's policy and grants.
        RelayMsg::HostGet
        | RelayMsg::HostDo { .. }
        | RelayMsg::ConfigGet
        | RelayMsg::ClipList
        | RelayMsg::ClipGet { .. }
        | RelayMsg::ClipPush { .. } => {
            let _ = crate::control::serve::handle(text, who, cx).await;
        }
        // The device's passkey: its own, whichever link it is on — the room's envelope proved the
        // key the unlock is bound to. Answered here, never dropped (Decision Log `2026-10-30`).
        msg @ (RelayMsg::PasskeyBegin { .. }
        | RelayMsg::PasskeyRegister { .. }
        | RelayMsg::PasskeyUnlock { .. }
        | RelayMsg::PasskeyLock
        | RelayMsg::PasskeyKeepalive
        | RelayMsg::PasskeyGet
        | RelayMsg::PasskeyStay { .. }) => {
            let dev = crate::passkey::relay::Device {
                key: &who.key,
                name: &who.name,
                ok: true,
            };
            crate::passkey::relay::handle(msg, dev, out).await;
        }
        RelayMsg::SessionStart { .. } => {
            let message = "this link joins a running session — start a new one from the \
                           Computer page"
                .to_string();
            send(out, &RelayMsg::SessionError { message }).await;
        }
        other => {
            let kind = crate::e2e::link::kind(text).unwrap_or("?").to_string();
            let message = if member() || matches!(other, RelayMsg::PtyOpen { .. }) {
                format!("not on a joined session's link ({kind}) — use it from your own")
            } else {
                "join the session first".to_string()
            };
            info!(device = %who.name, kind, "joined room: refused");
            send(out, &RelayMsg::SessionError { message }).await;
        }
    }
}
