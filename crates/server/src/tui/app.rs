//! The panel's state and what each key does. Drawing is [`super::view`]'s; side effects are
//! [`super::actions`]'s. Keys mean the same thing wherever they make sense — `f s c h` edit a
//! grant checklist, whichever one is in front of you.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::actions::{self, Outcome};
use super::data::Snapshot;
use crate::gate::{Grants, Verdict};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Rig,
    Sessions,
    Devices,
    Pair,
    Security,
}

impl Panel {
    pub const ALL: [Panel; 5] = [
        Panel::Rig,
        Panel::Sessions,
        Panel::Devices,
        Panel::Pair,
        Panel::Security,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Panel::Rig => "Rig",
            Panel::Sessions => "Sessions",
            Panel::Devices => "Devices",
            Panel::Pair => "Pair",
            Panel::Security => "Security",
        }
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// The rows of the Security panel, in order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Join,
    TrustFirst,
    NewDevice,
    Files,
    Shells,
}

pub const SETTINGS: [Setting; 5] = [
    Setting::Join,
    Setting::TrustFirst,
    Setting::NewDevice,
    Setting::Files,
    Setting::Shells,
];

pub enum Modal {
    /// `unpair <key>`, waiting for y.
    Unpair {
        key: String,
        name: String,
    },
    Help,
}

pub struct App {
    pub snap: Snapshot,
    pub focus: Panel,
    /// The cursor in each panel, by [`Panel::index`].
    pub cursor: [usize; 5],
    /// The checklist the next pairing code will carry.
    pub pair_grants: Grants,
    /// The last code made, and the checklist it was made with.
    pub pair_link: Option<(String, Grants)>,
    pub modal: Option<Modal>,
    /// Approval requests put off with Esc; they come back when a new one arrives.
    pub snoozed: Vec<String>,
    pub toast: Option<(Outcome, Instant)>,
    pub quit: bool,
    last_read: Instant,
}

impl App {
    pub fn new() -> Self {
        Self {
            snap: Snapshot::read(),
            focus: Panel::Devices,
            cursor: [0; 5],
            pair_grants: Grants::new_device(),
            pair_link: None,
            modal: None,
            snoozed: Vec::new(),
            toast: None,
            quit: false,
            last_read: Instant::now(),
        }
    }

    /// Re-read the world once a second.
    pub fn tick(&mut self) {
        if self.last_read.elapsed() >= Duration::from_secs(1) {
            self.refresh();
        }
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(5))
        {
            self.toast = None;
        }
    }

    fn refresh(&mut self) {
        self.snap = Snapshot::read();
        self.last_read = Instant::now();
        for p in Panel::ALL {
            let len = self.len(p);
            let c = &mut self.cursor[p.index()];
            *c = (*c).min(len.saturating_sub(1));
        }
    }

    fn done(&mut self, o: Outcome) {
        self.toast = Some((o, Instant::now()));
        self.refresh();
    }

    pub fn len(&self, p: Panel) -> usize {
        match p {
            // relay, tunnel, then each daemon
            Panel::Rig => 2 + self.snap.status.daemons.len(),
            Panel::Sessions => self.snap.status.sessions.len(),
            Panel::Devices => self.snap.devices.len(),
            Panel::Pair => 1,
            Panel::Security => SETTINGS.len(),
        }
    }

    pub fn at(&self, p: Panel) -> usize {
        self.cursor[p.index()]
    }

    /// The device waiting at the gate that has not been put off, if any.
    pub fn asking(&self) -> Option<&crate::gate::Request> {
        self.snap
            .pending
            .iter()
            .find(|r| !self.snoozed.contains(&r.id))
    }

    pub fn key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let Some(m) = self.modal.take() {
            return self.modal_key(m, k.code);
        }
        if let Some(r) = self.asking() {
            let (id, name) = (r.id.clone(), r.name.clone());
            let v = match k.code {
                KeyCode::Char('o') => Some(Verdict::Once),
                KeyCode::Char('a') => Some(Verdict::Always),
                KeyCode::Char('d') => Some(Verdict::Deny),
                KeyCode::Esc => {
                    self.snoozed.push(id);
                    return;
                }
                _ => None,
            };
            if let Some(v) = v {
                return self.done(actions::answer(&id, &name, v));
            }
            // Anything else falls through: a request must not lock the panel.
        }
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Char(c @ '1'..='5') => {
                self.focus = Panel::ALL[c as usize - '1' as usize];
            }
            KeyCode::Tab => self.focus = Panel::ALL[(self.focus.index() + 1) % 5],
            KeyCode::BackTab => self.focus = Panel::ALL[(self.focus.index() + 4) % 5],
            KeyCode::Down | KeyCode::Char('j') => self.step(1),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1),
            KeyCode::Char('r') => self.done(Ok("refreshed".into())),
            KeyCode::Char(c @ ('f' | 's' | 'c' | 'h')) => self.grant_key(c),
            KeyCode::Char('u') | KeyCode::Delete if self.focus == Panel::Devices => {
                if let Some(row) = self.snap.devices.get(self.at(Panel::Devices)) {
                    self.modal = Some(Modal::Unpair {
                        key: row.device.key.clone(),
                        name: row.device.name.clone(),
                    });
                }
            }
            KeyCode::Char('o') if self.focus == Panel::Devices => {
                if let Some(row) = self.snap.devices.get(self.at(Panel::Devices)) {
                    let (key, name) = (row.device.key.clone(), row.device.name.clone());
                    self.done(
                        actions::set("security.owner", &key)
                            .map(|_| format!("{name} is the owner now")),
                    );
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            _ => {}
        }
    }

    fn modal_key(&mut self, m: Modal, code: KeyCode) {
        match (m, code) {
            (Modal::Unpair { key, .. }, KeyCode::Char('y')) => self.done(actions::unpair(&key)),
            (Modal::Help, _) | (Modal::Unpair { .. }, _) => {}
        }
    }

    fn step(&mut self, by: isize) {
        let len = self.len(self.focus);
        if len == 0 {
            return;
        }
        let i = self.focus.index();
        self.cursor[i] = (self.cursor[i] as isize + by).rem_euclid(len as isize) as usize;
    }

    /// `f s c h`: flip a grant on whichever checklist is in front of you.
    fn grant_key(&mut self, c: char) {
        let flip = |g: &Grants| -> (String, bool) {
            match c {
                // none → ro → rw → none
                'f' => match g.files {
                    "none" => ("files-ro".into(), true),
                    "ro" => ("files-rw".into(), true),
                    _ => ("files".into(), false),
                },
                's' => ("shells".into(), !g.shells),
                'c' => ("settings".into(), !g.settings),
                _ => ("host".into(), !g.host),
            }
        };
        match self.focus {
            Panel::Devices => {
                let Some(row) = self.snap.devices.get(self.at(Panel::Devices)) else {
                    return;
                };
                if row.owner && c != 'f' {
                    return self.done(Err("the owner may do everything but files already".into()));
                }
                let (token, on) = flip(&row.device.grants);
                let key = row.device.key.clone();
                self.done(actions::toggle(&key, &token, on));
            }
            Panel::Pair => {
                let (token, on) = flip(&self.pair_grants);
                let _ = self.pair_grants.set(&token, on);
            }
            Panel::Security if SETTINGS[self.at(Panel::Security)] == Setting::NewDevice => {
                let mut g = Grants::new_device();
                let (token, on) = flip(&g);
                let _ = g.set(&token, on);
                let v = g.tokens();
                let v = if v.is_empty() { "-".into() } else { v };
                self.done(actions::set("security.new-device", &v));
            }
            _ => {}
        }
    }

    /// Enter / space on the selected row.
    fn activate(&mut self) {
        match self.focus {
            Panel::Pair => {
                let g = self.pair_grants.clone();
                match actions::pair(&g) {
                    Ok(link) => {
                        self.pair_link = Some((link, g));
                        self.toast = Some((
                            Ok("new pairing code — single use, good for a day".into()),
                            Instant::now(),
                        ));
                    }
                    Err(e) => self.done(Err(e)),
                }
            }
            Panel::Security => {
                let s = &self.snap.cfg;
                let o = match SETTINGS[self.at(Panel::Security)] {
                    Setting::Join => actions::set(
                        "security.join",
                        if s.security.open_join() {
                            "ask"
                        } else {
                            "open"
                        },
                    ),
                    Setting::TrustFirst => actions::set(
                        "security.trust-first-device",
                        bool_kdl(!s.security.trust_first_device),
                    ),
                    Setting::Files => actions::set("files.enabled", bool_kdl(!s.files.enabled)),
                    Setting::Shells => actions::set("shells.enabled", bool_kdl(!s.shells.enabled)),
                    Setting::NewDevice => Err("f s c h edit what a new device may do".into()),
                };
                self.done(o);
            }
            _ => {}
        }
    }
}

fn bool_kdl(b: bool) -> &'static str {
    if b { "#true" } else { "#false" }
}
