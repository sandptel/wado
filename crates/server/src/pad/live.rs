//! The open pad of the phone on the relay link, whichever way its messages arrive.
//!
//! One [`Pad`] serves both transports, so a phone can begin on the relay and move to its direct
//! link mid-drag without a ghost finger. Touch frames are written on the pad's tick, every
//! `trackpad { rate-hz }`, by the pacer (`pacer.rs`) — or as they land, on the direct link with
//! `trackpad { smoothing #false }`. The tick parks while the pad is idle (`ticker.rs`); every
//! message applied wakes it. Keyboard messages go through one ordered queue, so text typed
//! by `wtype` can never be overtaken by the Enter pressed after it. Every message is checked
//! against the gate first; a refusal answers why and closes the pad.
//!
//! The gamepad (`game.rs`) lives beside the pad, not in it: it needs no touchpad or keyboard,
//! and it outlives a closed pad by its grace, on a clock and a timer of its own.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{info, warn};
use webrtc::api::API;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use webrtc::peer_connection::RTCPeerConnection;

use wado_platform::Capability;
use wado_protocol::pad::PadMsg;
use wado_protocol::relay::RelayMsg;

use super::game::Game;
use super::sink::Devices;
use super::state::Pad;
use super::ticker::{self, Pace};
use crate::files::channel::Who;
use crate::gate::Gate;

/// The device on the other end, and whether it may be here — asked again on every message.
#[derive(Clone)]
pub struct Peer {
    pub who: Who,
    pub gate: Gate,
    pub approved: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct Live {
    inner: Arc<Mutex<Inner>>,
    /// Signalled after every message the pad takes, so a parked tick resumes.
    wake: Arc<Notify>,
}

#[derive(Default)]
struct Inner {
    pad: Option<Pad>,
    epoch: Option<Instant>,
    tasks: Vec<JoinHandle<()>>,
    keys: Option<mpsc::UnboundedSender<PadMsg>>,
    pc: Option<Arc<RTCPeerConnection>>,
    keys_dc: Option<Arc<RTCDataChannel>>,
    relay: Option<mpsc::Sender<String>>,
    /// Bumped by every `close`, so an `open` that was still gathering when the pad closed
    /// throws its peer connection away instead of keeping it.
    generation: u64,
    /// The gamepad, made on first use and kept across closes for its grace.
    game: Option<Game>,
    /// The gamepad's clock, from its first use; never reset, so a grace survives a close.
    game_epoch: Option<Instant>,
    /// The gamepad's timer, while anything about it is timed.
    game_task: Option<JoinHandle<()>>,
    game_wake: Arc<Notify>,
}

impl Live {
    /// Answer the phone's offer for its direct link. The relay path needs no setup.
    pub async fn open(
        &self,
        api: &API,
        offer: &str,
        peer: Peer,
        relay: mpsc::Sender<String>,
    ) -> Result<String, String> {
        let generation = {
            let mut inner = self.lock();
            inner.relay = Some(relay);
            inner.generation
        };
        if let Some(why) = super::gate::refused(&peer.gate, &peer.who, approved(&peer))
            .or_else(|| super::gate::step_up(&peer.who, true))
        {
            return Err(why);
        }
        let (pc, answer) = super::channel::open(api, offer, self.clone(), peer)
            .await
            .map_err(|e| e.to_string())?;
        let old = {
            let mut inner = self.lock();
            if inner.generation != generation {
                Some(pc) // closed while gathering: this link is already unwanted
            } else {
                inner.pc.replace(pc)
            }
        };
        if let Some(old) = old {
            tokio::spawn(async move {
                let _ = old.close().await;
            });
        }
        Ok(answer)
    }

    /// A message that came through the relay.
    pub fn relayed(&self, msg: PadMsg, peer: &Peer, relay: &mpsc::Sender<String>) {
        self.lock().relay = Some(relay.clone());
        self.accept(msg, true, peer);
    }

    /// The direct link's keys channel opened (replies go there while it is open).
    pub(super) fn keys_opened(&self, dc: Arc<RTCDataChannel>) {
        self.lock().keys_dc = Some(dc);
    }

    /// That keys channel closed. Keys it held are let go — the phone, if it carries on over the
    /// relay, says again what it holds. A channel already replaced by a newer one changes nothing.
    pub(super) fn keys_closed(&self, dc: &Arc<RTCDataChannel>) {
        let mut inner = self.lock();
        if inner.keys_dc.as_ref().is_some_and(|k| Arc::ptr_eq(k, dc)) {
            inner.keys_dc = None;
            if let Some(pad) = inner.pad.as_mut() {
                pad.release_keys();
            }
        }
    }

    /// One message from the phone, by either path.
    pub(super) fn accept(&self, msg: PadMsg, relayed: bool, peer: &Peer) {
        let Some(msg) = super::bin::unpack(msg) else {
            warn!(device = %peer.who.name, "pad: an unreadable compact frame — dropped");
            return;
        };
        if msg == PadMsg::Bye {
            self.close();
            return;
        }
        // Gated on `host`, not `input`, and need no pad devices.
        if matches!(msg, PadMsg::Actions | PadMsg::Run { .. }) {
            super::actions::handle(self, msg, peer);
            return;
        }
        self.game_heard();
        // Letting go is never refused.
        if msg == PadMsg::GameOff {
            self.game(msg);
            return;
        }
        let acting = !matches!(msg, PadMsg::Ping { .. });
        if let Some(why) = super::gate::refused(&peer.gate, &peer.who, approved(peer))
            .or_else(|| super::gate::step_up(&peer.who, acting))
        {
            info!(device = %peer.who.name, "pad: refused — {why}");
            self.fail(why);
            return;
        }
        if matches!(
            msg,
            PadMsg::GameOn | PadMsg::Btn { .. } | PadMsg::Axes { .. }
        ) {
            self.game(msg);
            return;
        }
        let keyboard = matches!(
            msg,
            PadMsg::Key { .. } | PadMsg::Char { .. } | PadMsg::Hold { .. } | PadMsg::Text { .. }
        );
        if !self.start(&peer.who.name, &peer.who.key, relayed) {
            return;
        }
        if keyboard {
            if let Some(keys) = self.lock().keys.as_ref() {
                let _ = keys.send(msg);
            }
            return;
        }
        let reply = self
            .with_pad(|pad, now| pad.handle(now, msg, relayed))
            .flatten();
        self.wake.notify_one();
        if let Some(reply) = reply {
            self.answer(reply);
        }
    }

    /// Tell the phone why, and close: it stops sending when `fatal` arrives.
    fn fail(&self, message: String) {
        self.reply(PadMsg::Err {
            message,
            fatal: true,
        });
        self.close();
    }

    /// A reply from the pad; a device that cannot be made is not retried 60 times a second.
    fn answer(&self, reply: PadMsg) {
        match reply {
            PadMsg::Err {
                message,
                fatal: true,
            } => self.fail(message),
            reply => self.reply(reply),
        }
    }

    /// Let go of everything, destroy the devices, close the direct link. The gamepad lets go
    /// too, and goes after its grace.
    pub fn close(&self) {
        self.game(PadMsg::GameOff);
        let mut inner = self.lock();
        if let Some(mut pad) = inner.pad.take() {
            pad.release_all();
            info!("pad: closed");
        }
        for t in inner.tasks.drain(..) {
            t.abort();
        }
        inner.keys = None;
        inner.keys_dc = None;
        inner.epoch = None;
        inner.generation += 1;
        if let Some(pc) = inner.pc.take() {
            tokio::spawn(async move {
                let _ = pc.close().await;
            });
        }
    }

    /// Make the pad, its devices and its two workers on the first message that passed the gate.
    /// False when the devices cannot be made (the phone has been told why).
    fn start(&self, device: &str, key: &str, relayed: bool) -> bool {
        let mut inner = self.lock();
        if inner.pad.is_some() {
            return true;
        }
        let mut pad = Pad::new(devices());
        let key = key.to_string();
        pad.set_speed(move || speed(&key));
        if let Err(e) = pad.prepare() {
            let message = e.to_string();
            drop(inner);
            warn!(device, "pad: {message}");
            self.fail(message);
            return false;
        }
        info!(
            device,
            transport = if relayed { "relay" } else { "direct" },
            "pad: open"
        );
        inner.pad = Some(pad);
        inner.epoch = Some(Instant::now());

        // How often touch frames are written and silent fingers checked, while anything is in
        // flight; parked otherwise (`ticker.rs`).
        let tick = super::pacer::period(wado_config::live::current().trackpad.rate_hz);
        let ticker = {
            let live = self.clone();
            let epoch = inner.epoch.unwrap_or_else(Instant::now);
            tokio::spawn(ticker::run(tick, epoch, self.wake.clone(), move || {
                let (err, pace) = live
                    .with_pad(|pad, now| (pad.tick(now), pad.pace()))
                    .unwrap_or((None, Pace::Park(None)));
                if let Some(err) = err {
                    live.answer(err);
                }
                pace
            }))
        };

        let (tx, mut rx) = mpsc::unbounded_channel::<PadMsg>();
        let typist = {
            let live = self.clone();
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let (reply, text) = live
                        .with_pad(|pad, now| (pad.handle(now, msg, false), pad.take_text()))
                        .unwrap_or((None, None));
                    live.wake.notify_one();
                    if let Some(reply) = reply {
                        live.answer(reply);
                    }
                    if let Some(text) = text
                        && let Err(message) = type_text(text).await
                    {
                        warn!("pad: typing failed — {message}");
                        live.reply(PadMsg::Err {
                            message,
                            fatal: false,
                        });
                    }
                }
            })
        };
        inner.keys = Some(tx);
        inner.tasks = vec![ticker, typist];
        true
    }

    /// One gamepad message, already through the gate. The device is made on first use; its
    /// reply (the player number, or why it cannot be made) goes back to the phone.
    fn game(&self, msg: PadMsg) {
        let reply = {
            let mut inner = self.lock();
            let alive = inner.game.as_ref().is_some_and(Game::alive);
            if msg == PadMsg::GameOff && !alive {
                return; // nothing to let go of
            }
            let now = ticker::ms_since(*inner.game_epoch.get_or_insert_with(Instant::now));
            // No device yet: look for the platform's again, which may have been fixed since.
            if !alive {
                let silence = wado_config::live::current().trackpad.key_silence_ms;
                inner.game = Some(Game::from_platform(silence));
            }
            let reply = inner.game.as_mut().and_then(|g| g.handle(now, msg));
            self.game_timer(&mut inner);
            reply
        };
        if let Some(reply) = reply {
            self.answer(reply);
        }
    }

    /// Anything arrived: the phone is still there, so held buttons stay held.
    fn game_heard(&self) {
        let mut inner = self.lock();
        let Some(epoch) = inner.game_epoch else {
            return;
        };
        if let Some(game) = inner.game.as_mut() {
            game.heard(ticker::ms_since(epoch));
        }
    }

    /// Make sure the gamepad's timed rules run: a task sleeping until the next one, woken when a
    /// message moves it. It ends itself when nothing is timed.
    fn game_timer(&self, inner: &mut Inner) {
        if inner.game.as_ref().and_then(Game::due).is_none() {
            return;
        }
        if inner.game_task.is_some() {
            inner.game_wake.notify_one(); // its next deadline may have moved
            return;
        }
        let (live, wake) = (self.clone(), inner.game_wake.clone());
        let epoch = *inner.game_epoch.get_or_insert_with(Instant::now);
        inner.game_task = Some(tokio::spawn(async move {
            loop {
                let next = {
                    let mut inner = live.lock();
                    let now = ticker::ms_since(epoch);
                    let next = inner.game.as_mut().and_then(|g| {
                        g.tick(now);
                        g.due()
                    });
                    if next.is_none() {
                        inner.game_task = None;
                        return;
                    }
                    next
                };
                if let Some(ms) = next {
                    tokio::select! {
                        () = wake.notified() => {}
                        () = tokio::time::sleep_until(epoch + std::time::Duration::from_millis(ms)) => {}
                    }
                }
            }
        }));
    }

    /// Run `f` on the open pad with the current pad-clock time; `None` when there is no pad.
    fn with_pad<T>(&self, f: impl FnOnce(&mut Pad, u64) -> T) -> Option<T> {
        let mut inner = self.lock();
        let now = ticker::ms_since(inner.epoch?);
        inner.pad.as_mut().map(|pad| f(pad, now))
    }

    /// Answer the phone on its direct link while it is open, else through the relay.
    pub(super) fn reply(&self, msg: PadMsg) {
        let inner = self.lock();
        if let Some(dc) = inner
            .keys_dc
            .as_ref()
            .filter(|dc| dc.ready_state() == RTCDataChannelState::Open)
        {
            let dc = Arc::clone(dc);
            if let Ok(text) = serde_json::to_string(&msg) {
                tokio::spawn(async move {
                    let _ = dc.send_text(text).await;
                });
            }
            return;
        }
        if let Some(relay) = inner.relay.as_ref()
            && let Ok(text) = serde_json::to_string(&RelayMsg::PadOut { msg })
        {
            let _ = relay.try_send(text);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The platform's touchpad and keyboard. Missing ones are looked for again first: a pad opening
/// is rare, and `/dev/uinput` may have been fixed since the daemon started.
fn devices() -> Devices {
    let p = crate::platform::get();
    for cap in [Capability::Touch, Capability::Keys] {
        if !p.has(cap) {
            p.reprobe(cap);
        }
    }
    Devices::from_platform(p)
}

/// Type committed text on the desktop through the platform — looked for again when missing,
/// so a typist installed since the daemon started is used.
async fn type_text(text: String) -> Result<(), String> {
    let p = crate::platform::get();
    if !p.has(Capability::Text) {
        p.reprobe(Capability::Text);
    }
    match p.text() {
        Ok(t) => t.type_text(text).await,
        Err(absent) => Err(absent.why),
    }
}

/// This phone's pointer speed as the config has it now (`device "<key>" { trackpad { speed } }`).
fn speed(key: &str) -> f64 {
    wado_config::live::current()
        .device
        .get(key)
        .and_then(|d| d.trackpad.as_ref())
        .map_or(1.0, |t| t.speed)
}

fn approved(peer: &Peer) -> bool {
    peer.approved.load(Ordering::SeqCst)
}
