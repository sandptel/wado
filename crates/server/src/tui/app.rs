//! The panel's state and what each key does. Drawing is [`super::view`]'s; side effects are
//! [`super::actions`]'s.
//!
//! One rule everywhere: **tab** picks a card, **arrows** move in it, **space** flips what is under
//! the cursor — and a click does both. Grants read the same way wherever they appear: a row of
//! four toggles, `files shells settings host`.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::actions::{self, Outcome};
use super::data::Snapshot;
use super::load::Load;
use crate::gate::{Gate, Grants, Verdict};

/// The cards that take the cursor, in tab order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Card {
    Pair,
    Devices,
    Switches,
}

impl Card {
    pub const ALL: [Card; 3] = [Card::Pair, Card::Devices, Card::Switches];
    fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

/// The rows of the Switches card, in order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Join,
    TrustFirst,
    Files,
    Shells,
    NewDevice,
}

pub const SWITCHES: [Switch; 5] = [
    Switch::Join,
    Switch::TrustFirst,
    Switch::Files,
    Switch::Shells,
    Switch::NewDevice,
];

/// The grant columns, in the order every checklist shows them.
pub const GRANTS: [&str; 5] = ["files", "shells", "settings", "host", "clipboard"];
/// The Devices matrix: the grants, then who is owner.
pub const OWNER_COL: usize = 5;

/// What a click on a drawn region does.
#[derive(Clone, Copy)]
pub enum Target {
    Cell(Card, usize, usize),
    Answer(Verdict),
    Snooze,
    /// The QR code: compact ↔ large.
    Zoom,
}

/// The pairing code on screen.
pub struct Pair {
    pub link: String,
    pub code: String,
}

pub struct App {
    pub snap: Snapshot,
    pub load: Load,
    pub focus: Card,
    /// `(row, column)` in each card, by [`Card::index`].
    pub cursor: [(usize, usize); 3],
    /// The checklist the QR on screen carries.
    pub pair_grants: Grants,
    pub pair: Result<Pair, String>,
    /// The QR at its large size, for a camera that struggles with the compact one.
    pub big_qr: bool,
    /// `(key, name)` of a device waiting for `y` to be unpaired.
    pub confirm: Option<(String, String)>,
    /// Approval requests put off with Esc; they come back when a new one arrives.
    pub snoozed: Vec<String>,
    pub toast: Option<(Outcome, Instant)>,
    pub quit: bool,
    /// Where the last frame put each clickable thing.
    pub hits: RefCell<Vec<(Rect, Target)>>,
    last_read: Instant,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            snap: Snapshot::read(),
            load: Load::default(),
            focus: Card::Pair,
            cursor: [(0, 0); 3],
            pair_grants: Grants::new_device(),
            pair: Err("making a code…".into()),
            big_qr: false,
            confirm: None,
            snoozed: Vec::new(),
            toast: None,
            quit: false,
            hits: RefCell::new(Vec::new()),
            last_read: Instant::now(),
        };
        app.load.sample();
        app.mint();
        app
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
        self.load.sample();
        self.last_read = Instant::now();
        // The QR on screen is always one that works: a used or expired code is replaced.
        match &self.pair {
            Ok(p) if !Gate::default().pair_live(&p.code) => {
                self.mint();
                self.toast = Some((
                    Ok("the code was used — a fresh one is up".into()),
                    Instant::now(),
                ));
            }
            Err(_) => self.mint(),
            Ok(_) => {}
        }
        for c in Card::ALL {
            let rows = self.rows(c);
            let (r, col) = self.cursor[c.index()];
            let r = r.min(rows.saturating_sub(1));
            self.cursor[c.index()] = (r, col.min(self.cols(c, r).saturating_sub(1)));
        }
    }

    /// Replace the code on screen with one carrying the current checklist.
    fn mint(&mut self) {
        if let Ok(old) = &self.pair {
            Gate::default().revoke_pair(&old.code);
        }
        self.pair = actions::pair(&self.pair_grants).map(|link| Pair {
            code: link
                .split("pair=")
                .nth(1)
                .and_then(|r| r.split('&').next())
                .unwrap_or_default()
                .to_string(),
            link,
        });
    }

    fn done(&mut self, o: Outcome) {
        self.toast = Some((o, Instant::now()));
        self.refresh();
    }

    pub fn rows(&self, c: Card) -> usize {
        match c {
            Card::Pair => 1,
            Card::Devices => self.snap.devices.len(),
            Card::Switches => SWITCHES.len(),
        }
    }

    fn cols(&self, c: Card, row: usize) -> usize {
        match c {
            Card::Pair => GRANTS.len(),
            Card::Devices => GRANTS.len() + 1,
            Card::Switches if SWITCHES.get(row) == Some(&Switch::NewDevice) => GRANTS.len(),
            Card::Switches => 1,
        }
    }

    /// The cursor of card `c`, or `None` when it is not the focused one.
    pub fn at(&self, c: Card) -> Option<(usize, usize)> {
        (self.focus == c).then(|| self.cursor[c.index()])
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
        if let Some((key, _)) = self.confirm.take() {
            if k.code == KeyCode::Char('y') {
                self.done(actions::unpair(&key));
            }
            return;
        }
        if self.asking().is_some() {
            let t = match k.code {
                KeyCode::Char('y' | 'a') => Some(Target::Answer(Verdict::Always)),
                KeyCode::Char('o') => Some(Target::Answer(Verdict::Once)),
                KeyCode::Char('n' | 'd') => Some(Target::Answer(Verdict::Deny)),
                KeyCode::Esc => Some(Target::Snooze),
                _ => None,
            };
            if let Some(t) = t {
                return self.hit(t);
            }
            // Anything else falls through: a request must not lock the panel.
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Tab => self.focus = Card::ALL[(self.focus.index() + 1) % 3],
            KeyCode::BackTab => self.focus = Card::ALL[(self.focus.index() + 2) % 3],
            KeyCode::Down | KeyCode::Char('j') => self.step(1, 0),
            KeyCode::Up | KeyCode::Char('k') => self.step(-1, 0),
            KeyCode::Right | KeyCode::Char('l') => self.step(0, 1),
            KeyCode::Left | KeyCode::Char('h') => self.step(0, -1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let (r, c) = self.cursor[self.focus.index()];
                self.flip(self.focus, r, c);
            }
            KeyCode::Char('x') | KeyCode::Delete if self.focus == Card::Devices => {
                if let Some(row) = self.snap.devices.get(self.cursor[1].0) {
                    self.confirm = Some((row.device.key.clone(), row.device.name.clone()));
                }
            }
            KeyCode::Char('z') => self.big_qr = !self.big_qr,
            KeyCode::Char('c') => match &self.pair {
                Ok(p) => {
                    actions::copy(&p.link);
                    self.toast = Some((Ok("link copied".into()), Instant::now()));
                }
                Err(e) => self.toast = Some((Err(e.clone()), Instant::now())),
            },
            _ => {}
        }
    }

    pub fn mouse(&mut self, m: MouseEvent) {
        match m.kind {
            MouseEventKind::Down(_) => {
                let at = Position::new(m.column, m.row);
                let hit = self
                    .hits
                    .borrow()
                    .iter()
                    .find(|(r, _)| r.contains(at))
                    .map(|(_, t)| *t);
                if let Some(t) = hit {
                    self.hit(t);
                }
            }
            MouseEventKind::ScrollDown => self.scroll(1),
            MouseEventKind::ScrollUp => self.scroll(-1),
            _ => {}
        }
    }

    fn hit(&mut self, t: Target) {
        match t {
            Target::Cell(card, r, c) => {
                self.focus = card;
                self.cursor[card.index()] = (r, c);
                self.flip(card, r, c);
            }
            Target::Answer(v) => {
                if let Some(req) = self.asking() {
                    let (id, name) = (req.id.clone(), req.name.clone());
                    self.done(actions::answer(&id, &name, v));
                }
            }
            Target::Zoom => self.big_qr = !self.big_qr,
            Target::Snooze => {
                if let Some(req) = self.asking() {
                    self.snoozed.push(req.id.clone());
                }
            }
        }
    }

    /// Move the cursor; past a card's edge it crosses to the card on that side —
    /// Pair on the left, Devices above Switches on the right.
    fn step(&mut self, dr: isize, dc: isize) {
        let card = self.focus;
        let rows = self.rows(card) as isize;
        let (r, c) = self.cursor[card.index()];
        let (r2, c2) = (r as isize + dr, c as isize + dc);
        let cols = self.cols(card, r) as isize;
        let across = match card {
            Card::Devices if rows == 0 => match (dr, dc) {
                (_, -1) => Some(Card::Pair),
                (1, _) => Some(Card::Switches),
                _ => None,
            },
            Card::Pair if c2 >= cols || r2 != r as isize => Some(Card::Devices),
            Card::Devices if c2 < 0 => Some(Card::Pair),
            Card::Devices if r2 >= rows => Some(Card::Switches),
            Card::Switches if r2 < 0 => Some(Card::Devices),
            Card::Switches if c2 < 0 => Some(Card::Pair),
            _ => None,
        };
        if let Some(to) = across {
            self.focus = to;
            return;
        }
        let r = r2.clamp(0, rows - 1) as usize;
        let c = c2.clamp(0, self.cols(card, r) as isize - 1) as usize;
        self.cursor[card.index()] = (r, c);
    }

    /// The wheel moves within the card, never out of it.
    fn scroll(&mut self, by: isize) {
        let card = self.focus;
        let rows = self.rows(card) as isize;
        if rows > 0 {
            let (r, c) = self.cursor[card.index()];
            let r = (r as isize + by).clamp(0, rows - 1) as usize;
            self.cursor[card.index()] = (r, c.min(self.cols(card, r) - 1));
        }
    }

    /// Space on `(row, col)` of `card`.
    fn flip(&mut self, card: Card, row: usize, col: usize) {
        match card {
            Card::Pair => {
                let (token, on) = flip(&self.pair_grants, col);
                let _ = self.pair_grants.set(&token, on);
                self.mint();
            }
            Card::Devices => {
                let Some(r) = self.snap.devices.get(row) else {
                    return;
                };
                let (key, name) = (r.device.key.clone(), r.device.name.clone());
                if col == OWNER_COL {
                    if r.owner {
                        return self.done(Ok(format!("{name} is already the owner")));
                    }
                    return self.done(
                        actions::set("security.owner", &key)
                            .map(|_| format!("{name} is the owner now")),
                    );
                }
                if r.owner && col != 0 {
                    return self.done(Err("the owner may do everything but files already".into()));
                }
                let (token, on) = flip(&r.device.grants, col);
                self.done(actions::toggle(&key, &token, on));
            }
            Card::Switches => {
                let s = &self.snap.cfg;
                let o = match SWITCHES[row] {
                    Switch::Join => actions::set(
                        "security.join",
                        if s.security.open_join() {
                            "ask"
                        } else {
                            "open"
                        },
                    ),
                    Switch::TrustFirst => actions::set(
                        "security.trust-first-device",
                        bool_kdl(!s.security.trust_first_device),
                    ),
                    Switch::Files => actions::set("files.enabled", bool_kdl(!s.files.enabled)),
                    Switch::Shells => actions::set("shells.enabled", bool_kdl(!s.shells.enabled)),
                    Switch::NewDevice => {
                        let mut g = Grants::new_device();
                        let (token, on) = flip(&g, col);
                        let _ = g.set(&token, on);
                        let v = g.tokens();
                        actions::set("security.new-device", if v.is_empty() { "-" } else { &v })
                    }
                };
                self.done(o);
            }
        }
    }
}

/// A code nobody can see any more is withdrawn.
impl Drop for App {
    fn drop(&mut self) {
        if let Ok(p) = &self.pair {
            Gate::default().revoke_pair(&p.code);
        }
    }
}

/// The grant token and new state for flipping column `col` of `g` — files step none → ro → rw.
fn flip(g: &Grants, col: usize) -> (String, bool) {
    match col {
        0 => match g.files {
            "none" => ("files-ro".into(), true),
            "ro" => ("files-rw".into(), true),
            _ => ("files".into(), false),
        },
        1 => ("shells".into(), !g.shells),
        2 => ("settings".into(), !g.settings),
        3 => ("host".into(), !g.host),
        _ => ("clipboard".into(), !g.clipboard),
    }
}

fn bool_kdl(b: bool) -> &'static str {
    if b { "#true" } else { "#false" }
}
