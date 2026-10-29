//! The open pad of the phone on the relay link, whichever way its messages arrive.
//!
//! One [`Pad`] serves both transports, so a phone can begin on the relay and move to its direct
//! link mid-drag without a ghost finger. Touch frames are applied the moment they land (or, when
//! relayed, when the playout says). Keyboard messages go through one ordered queue, so text typed
//! by `wtype` can never be overtaken by the Enter pressed after it. Every message is checked
//! against the gate first; a refusal answers why and closes the pad.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use webrtc::api::API;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;
use webrtc::peer_connection::RTCPeerConnection;

use wado_protocol::pad::PadMsg;
use wado_protocol::relay::RelayMsg;

use super::sink::Devices;
use super::state::Pad;
use crate::files::channel::Who;
use crate::gate::Gate;

/// How often relayed frames are released and silent fingers checked.
const TICK: Duration = Duration::from_millis(10);

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
        self.lock().relay = Some(relay);
        if let Some(why) = super::gate::refused(&peer.gate, &peer.who, approved(&peer)) {
            return Err(why);
        }
        let (pc, answer) = super::channel::open(api, offer, self.clone(), peer)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(old) = self.lock().pc.replace(pc) {
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
    pub(super) fn keys_channel(&self, dc: Option<Arc<RTCDataChannel>>) {
        self.lock().keys_dc = dc;
    }

    /// One message from the phone, by either path.
    pub(super) fn accept(&self, msg: PadMsg, relayed: bool, peer: &Peer) {
        if let Some(why) = super::gate::refused(&peer.gate, &peer.who, approved(peer)) {
            info!(device = %peer.who.name, "pad: refused — {why}");
            self.reply(PadMsg::Err { message: why });
            self.close();
            return;
        }
        let keyboard = matches!(
            msg,
            PadMsg::Key { .. } | PadMsg::Char { .. } | PadMsg::Hold { .. } | PadMsg::Text { .. }
        );
        self.start(&peer.who.name, relayed);
        if keyboard {
            if let Some(keys) = self.lock().keys.as_ref() {
                let _ = keys.send(msg);
            }
            return;
        }
        if let Some(reply) = self
            .with_pad(|pad, now| pad.handle(now, msg, relayed))
            .flatten()
        {
            self.reply(reply);
        }
    }

    /// Let go of everything, destroy the devices, close the direct link.
    pub fn close(&self) {
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
        if let Some(pc) = inner.pc.take() {
            tokio::spawn(async move {
                let _ = pc.close().await;
            });
        }
    }

    /// Make the pad and its two workers on the first message that passed the gate.
    fn start(&self, device: &str, relayed: bool) {
        let mut inner = self.lock();
        if inner.pad.is_some() {
            return;
        }
        info!(
            device,
            transport = if relayed { "relay" } else { "direct" },
            "pad: open"
        );
        inner.pad = Some(Pad::new(Box::new(Devices::default())));
        inner.epoch = Some(Instant::now());

        let ticker = {
            let live = self.clone();
            tokio::spawn(async move {
                let mut every = tokio::time::interval(TICK);
                every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    every.tick().await;
                    if let Some(err) = live.with_pad(|pad, now| pad.tick(now)).flatten() {
                        live.reply(err);
                    }
                }
            })
        };

        let (tx, mut rx) = mpsc::unbounded_channel::<PadMsg>();
        let typist = {
            let live = self.clone();
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let (reply, text) = live
                        .with_pad(|pad, now| (pad.handle(now, msg, false), pad.take_text()))
                        .unwrap_or((None, None));
                    if let Some(reply) = reply {
                        live.reply(reply);
                    }
                    if let Some(text) = text
                        && let Err(message) = super::text::type_text(&text).await
                    {
                        warn!("pad: typing failed — {message}");
                        live.reply(PadMsg::Err { message });
                    }
                }
            })
        };
        inner.keys = Some(tx);
        inner.tasks = vec![ticker, typist];
    }

    /// Run `f` on the open pad with the current pad-clock time; `None` when there is no pad.
    fn with_pad<T>(&self, f: impl FnOnce(&mut Pad, u64) -> T) -> Option<T> {
        let mut inner = self.lock();
        let now = inner.epoch?.elapsed().as_millis() as u64;
        inner.pad.as_mut().map(|pad| f(pad, now))
    }

    /// Answer the phone on its direct link while it is open, else through the relay.
    fn reply(&self, msg: PadMsg) {
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

fn approved(peer: &Peer) -> bool {
    peer.approved.load(Ordering::SeqCst)
}
