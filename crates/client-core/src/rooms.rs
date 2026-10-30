//! Rooms a UI opens itself, as the web client does (`relay_link.js`, `multi.js`, `side_link.js`):
//! a seat, a control room or a view room on one saved computer. The core keeps each one up —
//! handshake, seal, pacing, reconnect — and hands every opened message and every relay frame the
//! room's UI reads to the UI as it came, tagged with the room's id. The UI speaks the protocol;
//! the core is the secure pipe.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::event::{CoreEvent, LinkView};
use crate::link::driver::{Clock, LinkOut};
use crate::link::state::LinkState;
use crate::pins::{PinFrom, Pins};
use crate::queue::EventQueue;

/// One room's reports, as events.
pub struct RoomPump {
    pub id: String,
    pub remote_id: String,
    pub events: Arc<EventQueue>,
    pub pins: Arc<Mutex<Pins>>,
    pub clock: Clock,
}

impl RoomPump {
    pub async fn run(self, mut out: mpsc::UnboundedReceiver<LinkOut>) {
        let mut caps: Vec<String> = Vec::new();
        let mut instance = String::new();
        while let Some(o) = out.recv().await {
            match o {
                LinkOut::State(s) => {
                    let view = LinkView::new(s, (self.clock)(), caps.clone(), instance.clone());
                    self.events.push(CoreEvent::RoomState {
                        id: self.id.clone(),
                        view,
                    });
                }
                LinkOut::Joined {
                    caps: c,
                    instance: i,
                } => {
                    caps = c;
                    instance = i;
                }
                LinkOut::SavePin(pin) => {
                    let _ = self
                        .pins
                        .lock()
                        .unwrap()
                        .set(&self.remote_id, pin, PinFrom::FirstUse);
                }
                LinkOut::Raw(json) => self.events.push(CoreEvent::RoomMsg {
                    id: self.id.clone(),
                    json,
                }),
                LinkOut::Relay(json) => self.events.push(CoreEvent::RoomRelay {
                    id: self.id.clone(),
                    json,
                }),
                LinkOut::Msg(_) => {}
            }
        }
        // The driver returned: say so, once, so the UI never waits on a room that is gone.
        let view = LinkView::new(LinkState::Idle, (self.clock)(), caps, instance);
        self.events.push(CoreEvent::RoomState { id: self.id, view });
    }
}
