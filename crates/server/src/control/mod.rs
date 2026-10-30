//! Control-only links: a phone's widgets reaching this computer without a seat.
//!
//! A widget wants the computer's state and its controls — the volume, what is playing, the
//! clipboard, a quick action — but never its desktop, and it must never stand in another
//! device's way. So a widget's link is not a viewer: the relay (`?control=1`, its `control`
//! cap) gives it no seat and hands its frames to this daemon wrapped in `ctl` frames by room, and
//! this module serves each room on its own:
//!
//! - its own end-to-end envelope ([`crate::e2e::link::Link`]), the same handshake as a viewer's;
//! - only a device already trusted, proving the key on file — no pairing, no approval, ever;
//! - only the messages [`policy`] calls controls, each checked against the device's grants as
//!   it arrives ([`serve`]); nothing that starts, joins or drives a session, nor a shell, files
//!   or input.
//!
//! Nothing here touches the seat's viewer: its room, its peer connection, its envelope.

pub mod policy;
pub mod serve;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::relay::RelayMsg;

use crate::e2e::handshake::Codes;
use crate::e2e::host_key::HostKey;
use crate::e2e::link::{Inbound, Link, WAKE};
use crate::gate::{Admitted, Gate};

/// What every room shares: the gate, this computer's key, the compositor, and the relay link's
/// writer (`relay`), which passes `ctl` frames in the clear — each holds its room's own seal.
#[derive(Clone)]
pub struct Shared {
    pub gate: Gate,
    pub host_key: Arc<HostKey>,
    pub cmd_tx: Option<wado_compositor::CommandSender>,
    pub relay: mpsc::Sender<String>,
}

/// Sent down a room's writer after its last frame: close the link once that frame is out.
const CLOSE: &str = "\u{0}close";

struct Room {
    who: serve::Who,
    link: Arc<Mutex<Link>>,
    cx: serve::Ctx,
}

/// The control rooms on one relay connection. A new connection starts with none.
#[derive(Clone, Default)]
pub struct Rooms(Arc<Mutex<HashMap<String, Arc<Room>>>>);

impl Rooms {
    fn get(&self, room_id: &str) -> Option<Arc<Room>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(room_id)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A control-only client joined. Only a device this computer already trusts gets a room.
    pub async fn open(&self, sh: &Shared, room_id: String, key: String, name: String) {
        if !sh.gate.knows(&key) {
            info!(device = %name, "control link: refused — not a device this computer knows");
            let reason = crate::gate::UNPAIRED.to_string();
            let fail = wado_protocol::envelope::E2eMsg::E2eFail {
                reason: reason.clone(),
            };
            let text = serde_json::to_string(&fail).unwrap_or_default();
            send(
                &sh.relay,
                &RelayMsg::Ctl {
                    room_id: room_id.clone(),
                    text,
                },
            )
            .await;
            send(&sh.relay, &RelayMsg::CtlClose { room_id, reason }).await;
            return;
        }
        let mut link = Link::default();
        link.reset(&room_id, &key);
        let link = Arc::new(Mutex::new(link));
        let (out, rx) = mpsc::channel::<String>(64);
        tokio::spawn(writer(
            room_id.clone(),
            Arc::clone(&link),
            rx,
            sh.relay.clone(),
            self.clone(),
        ));
        let room = Room {
            who: serve::Who {
                key,
                name: name.clone(),
            },
            link,
            cx: serve::Ctx {
                gate: sh.gate.clone(),
                cmd_tx: sh.cmd_tx.clone(),
                sent: Arc::default(),
                clip: Arc::default(),
                out,
            },
        };
        info!(device = %name, "control link: opened (no seat, controls only)");
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(room_id, Arc::new(room));
    }

    /// One frame from a control client.
    pub async fn frame(&self, sh: &Shared, room_id: &str, text: &str) {
        let Some(room) = self.get(room_id) else {
            return;
        };
        let sorted = room
            .link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .inbound(text);
        match sorted {
            // A relay frame nested inside a client's: not the client's to send.
            Inbound::Relay => warn!("control link: dropped a relay frame posing as the client's"),
            Inbound::Drop(why) => warn!("control link: dropped a frame — {why}"),
            Inbound::Hello { v, eph } => {
                let reply = room.link.lock().unwrap_or_else(|e| e.into_inner()).hello(
                    &sh.host_key,
                    v,
                    &eph,
                );
                if let Some(m) = reply {
                    let _ = room
                        .cx
                        .out
                        .send(serde_json::to_string(&m).unwrap_or_default())
                        .await;
                }
            }
            // Any pairing code the client claims is ignored: pairing never happens here, and a
            // proof checked against no codes must not count as a wrong guess.
            Inbound::Finish { dev_pk, sig, .. } => {
                let done = room.link.lock().unwrap_or_else(|e| e.into_inner()).finish(
                    &dev_pk,
                    &sig,
                    "",
                    "",
                    &Codes::default(),
                );
                let verdict = done.and_then(|d| {
                    match sh
                        .gate
                        .admit(&room.who.key, &room.who.name, &dev_pk, &d.proof)
                    {
                        Ok(Admitted::In) => Ok(d),
                        Ok(Admitted::Confirm(_)) => Err("pairing is not done on a widget's link"),
                        Err(e) => Err(e),
                    }
                });
                let mut link = room.link.lock().unwrap_or_else(|e| e.into_inner());
                let out = match verdict {
                    Ok(d) => {
                        info!(device = %room.who.name, "control link: sealed");
                        link.open(d.sealer, d.opener);
                        vec![WAKE.to_string()]
                    }
                    Err(why) => {
                        warn!(device = %room.who.name, "control link: refused — {why}");
                        vec![
                            serde_json::to_string(&link.fail(why)).unwrap_or_default(),
                            CLOSE.to_string(),
                        ]
                    }
                };
                drop(link);
                for t in out {
                    let _ = room.cx.out.send(t).await;
                }
            }
            Inbound::Msg(m) => {
                if serve::handle(&m, &room.who, &room.cx).await.is_err() {
                    let _ = room.cx.out.send(CLOSE.to_string()).await;
                }
            }
        }
    }

    /// The relay connection ended: every room on it is gone.
    pub fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// The client left (or the relay says the room is gone): forget it.
    pub fn close(&self, room_id: &str) {
        if self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(room_id)
            .is_some()
        {
            info!("control link: closed");
        }
    }
}

/// A room's one writer: seals in order, wraps each frame for the relay, and ends the link when
/// told to — after everything before the close has gone.
async fn writer(
    room_id: String,
    link: Arc<Mutex<Link>>,
    mut rx: mpsc::Receiver<String>,
    relay: mpsc::Sender<String>,
    rooms: Rooms,
) {
    while let Some(text) = rx.recv().await {
        if text == CLOSE {
            rooms.close(&room_id);
            send(
                &relay,
                &RelayMsg::CtlClose {
                    room_id: room_id.clone(),
                    reason: String::new(),
                },
            )
            .await;
            return;
        }
        let frames = link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .outbound(text);
        for text in frames {
            let wrapped = RelayMsg::Ctl {
                room_id: room_id.clone(),
                text,
            };
            if !send(&relay, &wrapped).await {
                return;
            }
        }
    }
}

async fn send(tx: &mpsc::Sender<String>, msg: &RelayMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(t) => tx.send(t).await.is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests;
