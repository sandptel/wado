//! One watched computer: what its link reports, turned into events, and what it asks for while it
//! is up — the host state and the sessions list, at once and then every 5 s while the UI is on
//! screen (`visible`). Off screen nothing polls: the link stays a quiet, long-lived one. Its host
//! state also refreshes the profile's cached name and OS, so an offline card keeps them.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;
use wado_protocol::relay::RelayMsg;
use wado_protocol::relay_wire::display_remote_id;

use crate::event::{ComputerView, CoreEvent, LinkView};
use crate::feeds::{host, sessions};
use crate::link::driver::{Clock, LinkCmd, LinkOut};
use crate::link::outbox::Class;
use crate::link::state::LinkState;
use crate::pins::{PinFrom, Pins};
use crate::profiles::Profiles;
use crate::queue::EventQueue;

pub struct Pump {
    pub key: String,
    pub remote_id: String,
    pub cmds: mpsc::UnboundedSender<LinkCmd>,
    pub events: Arc<EventQueue>,
    pub profiles: Arc<Mutex<Profiles>>,
    pub pins: Arc<Mutex<Pins>>,
    pub clock: Clock,
    /// The UI is on screen: poll. Shared by every pump of the core.
    pub visible: Arc<AtomicBool>,
}

fn ask(cmds: &mpsc::UnboundedSender<LinkCmd>, msg: &RelayMsg) {
    if let Ok(text) = serde_json::to_string(msg) {
        // A poll is not worth resending after a reconnect: the next open asks afresh.
        let _ = cmds.send(LinkCmd::Send {
            text,
            class: Class::Urgent,
            resend: false,
        });
    }
}

impl Pump {
    fn poll(&self) {
        ask(&self.cmds, &RelayMsg::HostGet { ack: 0 });
        ask(&self.cmds, &RelayMsg::SessionsRequest);
    }

    /// Refresh the cached name and OS from a host view; true if anything changed.
    fn cache(&self, v: &host::HostView) -> bool {
        let mut ps = self.profiles.lock().unwrap();
        let Some(mut p) = ps.get(&self.key).cloned() else {
            return false;
        };
        let before = p.clone();
        // Named after its hostname until the person names it (P2), never after a blank.
        if !v.name.is_empty() && p.name == display_remote_id(&p.remote_id) {
            p.name = v.name.clone();
        }
        p.os_id = v.os_id.clone();
        p.os_like = v.os_like.clone();
        p.desktop = v.desktop.clone();
        if p == before {
            return false;
        }
        ps.upsert(p);
        let _ = ps.save();
        true
    }

    fn computers(&self) {
        let list = self
            .profiles
            .lock()
            .unwrap()
            .list()
            .iter()
            .map(ComputerView::of)
            .collect();
        self.events.push(CoreEvent::Computers { list });
    }

    /// Run until the link's reports end (it was stopped).
    pub async fn run(self, mut out: mpsc::UnboundedReceiver<LinkOut>) {
        let mut caps: Vec<String> = Vec::new();
        let mut instance = String::new();
        let mut open = false;
        let period = host::poll_ms(true).unwrap_or(5000);
        let mut tick = tokio::time::interval(Duration::from_millis(period));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                o = out.recv() => {
                    let Some(o) = o else { return };
                    match o {
                        LinkOut::State(s) => {
                            open = matches!(s, LinkState::Open { .. });
                            if open && self.visible.load(Ordering::Relaxed) {
                                self.poll();
                                tick.reset();
                            }
                            let view = LinkView::new(s, (self.clock)(), caps.clone(), instance.clone());
                            self.events.push(CoreEvent::Link { key: self.key.clone(), view });
                        }
                        LinkOut::Joined { caps: c, instance: i } => {
                            caps = c;
                            instance = i;
                        }
                        LinkOut::SavePin(pin) => {
                            let _ = self.pins.lock().unwrap().set(&self.remote_id, pin, PinFrom::FirstUse);
                        }
                        LinkOut::Msg(RelayMsg::HostState { state, .. }) => {
                            let view = host::view(&state);
                            if self.cache(&view) {
                                self.computers();
                            }
                            self.events.push(CoreEvent::Host { key: self.key.clone(), view });
                        }
                        LinkOut::Msg(RelayMsg::Sessions { sessions: list }) => {
                            let rows = sessions::rows(&list);
                            self.events.push(CoreEvent::Sessions { key: self.key.clone(), rows });
                        }
                        LinkOut::Msg(_) => {}
                    }
                }
                _ = tick.tick(), if open => {
                    if self.visible.load(Ordering::Relaxed) {
                        self.poll();
                    }
                }
            }
        }
    }
}
