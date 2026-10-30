//! The trackpad's touch surface, as a pure state machine: pointers in, full-state frames out,
//! paced exactly as the web client paces them (crates/client/src/js/pad_touch.js), so the
//! computer's libinput sees the same touchpad whichever client drives it.
//!
//! When a frame goes out:
//! - a finger lands or lifts → at once (a tap must never be coalesced away); on the direct link,
//!   whose touch channel never retransmits, the same frame again at +20 and +60 ms;
//! - a finger moves → direct link: at once; relayed: on a 16 ms tick;
//! - fingers rest → re-sent every 100 ms, so the daemon's 400 ms silence check only lifts a
//!   finger whose phone is really gone.
//!
//! The caller owns the clock: every call takes `now_ms` (monotonic), and [`PadPacer::poll`] is
//! called every few ms while the pad is up. Each call returns the messages to send now, as JSON
//! `wado_protocol::pad::PadMsg`, with `lossy` saying which channel (touch: lossy; size: keys).

use std::{collections::BTreeMap, sync::Mutex};

use wado_protocol::pad::{Contact, PadMsg};

/// Highest contact id; more fingers than this are ignored, as on the web pad.
const MAX_ID: u8 = 9;
const TICK_MS: u64 = 16;
const REST_MS: u64 = 100;
const REPEATS_MS: [u64; 2] = [20, 60];

/// One message to send: a `PadMsg` as JSON, and whether it rides the lossy touch channel.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PadOut {
    pub json: String,
    pub lossy: bool,
}

#[derive(Default)]
struct State {
    /// pointer id → (contact id, x, y), x/y normalised 0..=65535.
    fingers: BTreeMap<i64, (u8, u16, u16)>,
    seq: u32,
    dirty: bool,
    direct: bool,
    last_sent: u64,
    last_tick: u64,
    /// Repeats of a land/lift frame still due (direct link only).
    repeats: Vec<u64>,
    size: (u16, u16),
}

#[derive(uniffi::Object)]
pub struct PadPacer {
    s: Mutex<State>,
}

impl Default for PadPacer {
    fn default() -> Self {
        Self {
            s: Mutex::new(State::default()),
        }
    }
}

fn norm(v: f64, of: f64) -> u16 {
    if of <= 0.0 {
        return 0;
    }
    ((v / of).clamp(0.0, 1.0) * 65535.0).round() as u16
}

impl State {
    fn frame(&self) -> Vec<Contact> {
        let mut c: Vec<Contact> = self.fingers.values().copied().collect();
        c.sort_by_key(|f| f.0);
        c
    }

    fn send(&mut self, now: u64, out: &mut Vec<PadOut>) {
        self.seq = self.seq.wrapping_add(1);
        self.dirty = false;
        self.last_sent = now;
        let msg = PadMsg::Frame {
            seq: self.seq,
            t: now as u32,
            c: self.frame(),
        };
        out.push(PadOut {
            json: serde_json::to_string(&msg).unwrap_or_default(),
            lossy: true,
        });
    }

    fn size_msg(&self) -> Option<PadOut> {
        let (w, h) = self.size;
        (w > 0 && h > 0).then(|| PadOut {
            json: serde_json::to_string(&PadMsg::Size { w, h }).unwrap_or_default(),
            lossy: false,
        })
    }

    /// A finger landed or lifted: now, and again later on the direct link.
    fn changed(&mut self, now: u64, out: &mut Vec<PadOut>) {
        self.send(now, out);
        if self.direct {
            self.repeats = REPEATS_MS.iter().map(|d| now + d).collect();
        }
    }

    fn free_id(&self) -> Option<u8> {
        (0..=MAX_ID).find(|i| !self.fingers.values().any(|f| f.0 == *i))
    }
}

#[uniffi::export]
impl PadPacer {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::default()
    }

    /// The link's path: direct (a peer connection's lossy channel) or relayed.
    pub fn set_direct(&self, direct: bool) {
        self.s.lock().unwrap().direct = direct;
    }

    /// The pad's size in px (its own frame: landscape when held landscape). A change sends it.
    pub fn resize(&self, w: u16, h: u16) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        if s.size == (w, h) {
            return vec![];
        }
        s.size = (w, h);
        s.size_msg().into_iter().collect()
    }

    pub fn down(&self, pointer: i64, x: f64, y: f64, w: f64, h: f64, now_ms: u64) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        let mut out = vec![];
        let Some(id) = s.free_id() else {
            return out;
        };
        s.fingers.insert(pointer, (id, norm(x, w), norm(y, h)));
        s.changed(now_ms, &mut out);
        out
    }

    pub fn moved(&self, pointer: i64, x: f64, y: f64, w: f64, h: f64, now_ms: u64) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        let mut out = vec![];
        let Some(f) = s.fingers.get_mut(&pointer) else {
            return out;
        };
        let (nx, ny) = (norm(x, w), norm(y, h));
        if (f.1, f.2) == (nx, ny) {
            return out;
        }
        f.1 = nx;
        f.2 = ny;
        s.dirty = true;
        if s.direct {
            s.send(now_ms, &mut out);
        }
        out
    }

    pub fn up(&self, pointer: i64, now_ms: u64) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        let mut out = vec![];
        if s.fingers.remove(&pointer).is_some() {
            s.changed(now_ms, &mut out);
        }
        out
    }

    /// Every finger lifted at once (the pad turned, closed, or went to the background).
    pub fn lift_all(&self, now_ms: u64) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        let mut out = vec![];
        if !s.fingers.is_empty() {
            s.fingers.clear();
            s.changed(now_ms, &mut out);
        }
        out
    }

    /// The link moved paths: the daemon's picture of the pad is sent whole again.
    pub fn resend(&self, now_ms: u64) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        let mut out: Vec<PadOut> = s.size_msg().into_iter().collect();
        s.send(now_ms, &mut out);
        out
    }

    /// What is due now: the relayed tick, a land/lift repeat, or a resting re-send.
    pub fn poll(&self, now_ms: u64) -> Vec<PadOut> {
        let mut s = self.s.lock().unwrap();
        let mut out = vec![];
        let due = s.repeats.iter().any(|t| *t <= now_ms);
        if due {
            s.repeats.retain(|t| *t > now_ms);
            s.send(now_ms, &mut out);
            return out;
        }
        if s.dirty && !s.direct && now_ms.saturating_sub(s.last_tick) >= TICK_MS {
            s.last_tick = now_ms;
            s.send(now_ms, &mut out);
            return out;
        }
        if !s.fingers.is_empty() && now_ms.saturating_sub(s.last_sent) >= REST_MS {
            s.send(now_ms, &mut out);
        }
        out
    }

    /// How many fingers are down.
    pub fn fingers(&self) -> u32 {
        self.s.lock().unwrap().fingers.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(o: &PadOut) -> PadMsg {
        serde_json::from_str(&o.json).unwrap()
    }

    fn contacts(o: &PadOut) -> Vec<Contact> {
        match msg(o) {
            PadMsg::Frame { c, .. } => c,
            m => panic!("not a frame: {m:?}"),
        }
    }

    #[test]
    fn size_goes_once_on_the_keys_channel() {
        let p = PadPacer::new();
        let o = p.resize(800, 400);
        assert_eq!(o.len(), 1);
        assert!(!o[0].lossy);
        assert_eq!(msg(&o[0]), PadMsg::Size { w: 800, h: 400 });
        assert!(p.resize(800, 400).is_empty());
    }

    #[test]
    fn a_landing_goes_at_once_normalised() {
        let p = PadPacer::new();
        let o = p.down(7, 400.0, 100.0, 800.0, 400.0, 1000);
        assert_eq!(o.len(), 1);
        assert!(o[0].lossy);
        assert_eq!(contacts(&o[0]), vec![(0, 32768, 16384)]);
    }

    #[test]
    fn relayed_moves_wait_for_the_tick() {
        let p = PadPacer::new();
        p.down(1, 0.0, 0.0, 100.0, 100.0, 0);
        assert!(p.moved(1, 50.0, 50.0, 100.0, 100.0, 5).is_empty());
        let o = p.poll(20);
        assert_eq!(contacts(&o[0]), vec![(0, 32768, 32768)]);
        assert!(p.poll(25).is_empty(), "nothing new");
    }

    #[test]
    fn direct_moves_go_at_once_and_landings_repeat() {
        let p = PadPacer::new();
        p.set_direct(true);
        assert_eq!(p.down(1, 0.0, 0.0, 100.0, 100.0, 0).len(), 1);
        assert_eq!(p.moved(1, 10.0, 0.0, 100.0, 100.0, 5).len(), 1);
        assert!(p.poll(10).is_empty());
        assert_eq!(p.poll(20).len(), 1, "+20 repeat");
        assert!(p.poll(30).is_empty());
        assert_eq!(p.poll(60).len(), 1, "+60 repeat");
    }

    #[test]
    fn a_resting_finger_is_re_sent_every_100ms() {
        let p = PadPacer::new();
        p.down(1, 1.0, 1.0, 10.0, 10.0, 0);
        assert!(p.poll(50).is_empty());
        assert_eq!(p.poll(100).len(), 1);
        p.up(1, 120);
        assert!(p.poll(400).is_empty(), "nothing down, nothing re-sent");
    }

    #[test]
    fn ids_are_reused_lowest_first_and_capped() {
        let p = PadPacer::new();
        for i in 0..12 {
            p.down(i, 1.0, 1.0, 10.0, 10.0, 0);
        }
        assert_eq!(p.fingers(), 10);
        p.up(3, 1);
        let o = p.down(99, 1.0, 1.0, 10.0, 10.0, 2);
        assert!(contacts(&o[0]).iter().any(|c| c.0 == 3));
        let o = p.lift_all(3);
        assert!(contacts(&o[0]).is_empty());
    }

    #[test]
    fn frames_count_up() {
        let p = PadPacer::new();
        let a = p.down(1, 1.0, 1.0, 10.0, 10.0, 0);
        let b = p.up(1, 1);
        let seq = |o: &PadOut| match msg(o) {
            PadMsg::Frame { seq, .. } => seq,
            _ => 0,
        };
        assert_eq!(seq(&b[0]), seq(&a[0]) + 1);
    }
}
