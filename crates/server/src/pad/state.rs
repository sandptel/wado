//! One phone's pad: what it holds down, and what each message from it does.
//!
//! Shared by both transports, so a phone that starts on the relay and moves to the direct link
//! mid-drag keeps one continuous touch. Pure apart from the [`Sink`] it drives, which is what
//! lets the safety rules — stale frames dropped, a silent finger lifted, a resize lifting every
//! finger first — be tested without a kernel device.

use wado_protocol::pad::{Contact, Mod, PadMsg, newer};

use super::keys::{char_code, key_code};
use super::playout::Playout;

/// A finger down with no frame for this long is lifted: the phone is gone or its link is.
/// The phone re-sends its frame every 100 ms while a finger rests, so this never cuts a hold.
pub const SILENCE_MS: u64 = 400;

/// Where a pad's input lands. The real one is [`super::sink::Devices`].
pub trait Sink: Send {
    fn touch(&mut self, size: (u16, u16), c: &[Contact]) -> Result<(), String>;
    fn tap(&mut self, mods: &[Mod], code: u16) -> Result<(), String>;
    fn hold(&mut self, mods: &[Mod]) -> Result<(), String>;
    fn release_all(&mut self);
}

pub struct Pad {
    sink: Box<dyn Sink>,
    size: Option<(u16, u16)>,
    last_seq: Option<u32>,
    /// Any finger down, as last sent to the sink.
    down: bool,
    last_frame_ms: u64,
    playout: Playout,
    text: Vec<String>,
}

impl Pad {
    pub fn new(sink: Box<dyn Sink>) -> Pad {
        Pad {
            sink,
            size: None,
            last_seq: None,
            down: false,
            last_frame_ms: 0,
            playout: Playout::new(),
            text: Vec::new(),
        }
    }

    /// Apply one message that arrived at `now_ms`; `relayed` when it came through the relay.
    /// Returns the reply, if any.
    pub fn handle(&mut self, now_ms: u64, msg: PadMsg, relayed: bool) -> Option<PadMsg> {
        let result = match msg {
            PadMsg::Size { w, h } => {
                self.resize((w, h));
                Ok(())
            }
            PadMsg::Frame { seq, t, c } => {
                if self.last_seq.is_some_and(|last| !newer(seq, last)) {
                    return None;
                }
                self.last_seq = Some(seq);
                self.last_frame_ms = now_ms;
                let c = first_of_each_id(c);
                if relayed {
                    self.playout.push(now_ms, t, c);
                    return self.tick(now_ms);
                }
                self.touch(&c)
            }
            PadMsg::Key { key, mods } => match key_code(&key) {
                Some(code) => self.sink.tap(&mods, code),
                None => Err(format!("unknown key “{key}”")),
            },
            PadMsg::Char { ch, mut mods } => match char_code(ch) {
                Some((code, shift)) => {
                    if shift && !mods.contains(&Mod::Shift) {
                        mods.push(Mod::Shift);
                    }
                    self.sink.tap(&mods, code)
                }
                None => Err(format!(
                    "“{ch}” has no key on this keyboard layout — type it as text instead"
                )),
            },
            PadMsg::Hold { mods } => self.sink.hold(&mods),
            PadMsg::Text { text } => {
                self.text.push(text);
                Ok(())
            }
            PadMsg::Ping { t } => return Some(PadMsg::Pong { t }),
            // Daemon → phone only; a phone sending them is ignored.
            PadMsg::Pong { .. } | PadMsg::Err { .. } => Ok(()),
        };
        result.err().map(|message| PadMsg::Err { message })
    }

    /// Text waiting to be typed (typing is async; the caller does it).
    pub fn take_text(&mut self) -> Option<String> {
        (!self.text.is_empty()).then(|| self.text.remove(0))
    }

    /// Play relayed frames now due, and lift fingers that went silent.
    pub fn tick(&mut self, now_ms: u64) -> Option<PadMsg> {
        let mut err = None;
        for frame in self.playout.due(now_ms) {
            if let Err(e) = self.touch(&frame) {
                err.get_or_insert(e);
            }
        }
        if self.down && now_ms.saturating_sub(self.last_frame_ms) > SILENCE_MS {
            self.playout.clear();
            if let Err(e) = self.touch(&[]) {
                err.get_or_insert(e);
            }
        }
        err.map(|message| PadMsg::Err { message })
    }

    /// When `tick` next has work: the next relayed frame, or the silence deadline.
    pub fn next_wake(&self) -> Option<u64> {
        let silence = self.down.then_some(self.last_frame_ms + SILENCE_MS + 1);
        match (self.playout.next_wake(), silence) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Let go of everything: fingers, held keys.
    pub fn release_all(&mut self) {
        self.playout.clear();
        if self.down {
            let _ = self.touch(&[]);
        }
        self.sink.release_all();
    }

    fn touch(&mut self, c: &[Contact]) -> Result<(), String> {
        let Some(size) = self.size else {
            return Ok(()); // the phone always sends its size first; nothing to touch yet
        };
        self.down = !c.is_empty();
        self.sink.touch(size, c)
    }

    fn resize(&mut self, size: (u16, u16)) {
        let moved = |a: u16, b: u16| a.abs_diff(b) > 2;
        if let Some(old) = self.size {
            if !moved(old.0, size.0) && !moved(old.1, size.1) {
                return;
            }
            self.playout.clear();
            if self.down {
                let _ = self.touch(&[]);
            }
        }
        self.size = Some(size);
    }
}

/// A frame naming one finger twice keeps the first: a malformed frame must not drive one slot
/// from two places.
fn first_of_each_id(mut c: Vec<Contact>) -> Vec<Contact> {
    let mut seen = [false; 256];
    c.retain(|&(id, _, _)| !std::mem::replace(&mut seen[id as usize], true));
    c
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        Touch((u16, u16), Vec<Contact>),
        Tap(Vec<Mod>, u16),
        Hold(Vec<Mod>),
        Release,
    }

    #[derive(Clone, Default)]
    struct Rec(Arc<Mutex<Vec<Call>>>);
    impl Rec {
        fn calls(&self) -> Vec<Call> {
            self.0.lock().unwrap().clone()
        }
    }
    impl Sink for Rec {
        fn touch(&mut self, size: (u16, u16), c: &[Contact]) -> Result<(), String> {
            self.0.lock().unwrap().push(Call::Touch(size, c.to_vec()));
            Ok(())
        }
        fn tap(&mut self, mods: &[Mod], code: u16) -> Result<(), String> {
            self.0.lock().unwrap().push(Call::Tap(mods.to_vec(), code));
            Ok(())
        }
        fn hold(&mut self, mods: &[Mod]) -> Result<(), String> {
            self.0.lock().unwrap().push(Call::Hold(mods.to_vec()));
            Ok(())
        }
        fn release_all(&mut self) {
            self.0.lock().unwrap().push(Call::Release);
        }
    }

    fn pad() -> (Pad, Rec) {
        let rec = Rec::default();
        let mut p = Pad::new(Box::new(rec.clone()));
        p.handle(0, PadMsg::Size { w: 390, h: 800 }, false);
        (p, rec)
    }
    fn frame(seq: u32, c: Vec<Contact>) -> PadMsg {
        PadMsg::Frame {
            seq,
            t: seq * 16,
            c,
        }
    }

    #[test]
    fn stale_seq_is_dropped() {
        let (mut p, rec) = pad();
        p.handle(0, frame(5, vec![(0, 1, 1)]), false);
        p.handle(1, frame(4, vec![(0, 9, 9)]), false);
        assert_eq!(rec.calls(), vec![Call::Touch((390, 800), vec![(0, 1, 1)])]);
    }

    #[test]
    fn silence_lifts_all() {
        let (mut p, rec) = pad();
        p.handle(0, frame(1, vec![(0, 1, 1)]), false);
        p.tick(399);
        assert_eq!(rec.calls().len(), 1);
        p.tick(401);
        assert_eq!(rec.calls()[1], Call::Touch((390, 800), vec![]));
        p.tick(900);
        assert_eq!(rec.calls().len(), 2, "lifted once, not again every tick");
    }

    #[test]
    fn resize_lifts_then_recreates() {
        let (mut p, rec) = pad();
        p.handle(0, frame(1, vec![(0, 1, 1)]), false);
        p.handle(5, PadMsg::Size { w: 800, h: 390 }, false);
        p.handle(10, frame(2, vec![(0, 2, 2)]), false);
        assert_eq!(
            rec.calls(),
            vec![
                Call::Touch((390, 800), vec![(0, 1, 1)]),
                Call::Touch((390, 800), vec![]),
                Call::Touch((800, 390), vec![(0, 2, 2)]),
            ]
        );
    }

    #[test]
    fn a_tiny_resize_changes_nothing() {
        let (mut p, rec) = pad();
        p.handle(0, frame(1, vec![(0, 1, 1)]), false);
        p.handle(5, PadMsg::Size { w: 391, h: 798 }, false);
        assert_eq!(rec.calls().len(), 1);
    }

    #[test]
    fn ping_is_answered() {
        let (mut p, _) = pad();
        assert_eq!(
            p.handle(0, PadMsg::Ping { t: 5 }, false),
            Some(PadMsg::Pong { t: 5 })
        );
    }

    #[test]
    fn char_without_key_says_so() {
        let (mut p, _) = pad();
        let reply = p.handle(
            0,
            PadMsg::Char {
                ch: 'é',
                mods: vec![],
            },
            false,
        );
        assert!(matches!(reply, Some(PadMsg::Err { message }) if message.contains('é')));
    }

    #[test]
    fn a_shifted_char_adds_shift_once() {
        let (mut p, rec) = pad();
        p.handle(
            0,
            PadMsg::Char {
                ch: '!',
                mods: vec![Mod::Ctrl],
            },
            false,
        );
        p.handle(
            0,
            PadMsg::Char {
                ch: 'A',
                mods: vec![Mod::Shift],
            },
            false,
        );
        assert_eq!(
            rec.calls()[..],
            [
                Call::Tap(vec![Mod::Ctrl, Mod::Shift], 2),
                Call::Tap(vec![Mod::Shift], 30)
            ]
        );
    }

    #[test]
    fn unknown_key_says_so() {
        let (mut p, _) = pad();
        let reply = p.handle(
            0,
            PadMsg::Key {
                key: "hyper".into(),
                mods: vec![],
            },
            false,
        );
        assert!(matches!(reply, Some(PadMsg::Err { message }) if message.contains("hyper")));
    }

    #[test]
    fn relayed_frames_wait_for_playout() {
        let (mut p, rec) = pad();
        p.handle(
            1000,
            PadMsg::Frame {
                seq: 1,
                t: 0,
                c: vec![(0, 1, 1)],
            },
            true,
        );
        assert_eq!(rec.calls().len(), 0);
        assert_eq!(p.next_wake(), Some(1024));
        p.tick(1024);
        assert_eq!(rec.calls(), vec![Call::Touch((390, 800), vec![(0, 1, 1)])]);
    }

    #[test]
    fn a_doubled_finger_keeps_the_first() {
        let (mut p, rec) = pad();
        p.handle(0, frame(1, vec![(0, 1, 1), (0, 9, 9)]), false);
        assert_eq!(rec.calls(), vec![Call::Touch((390, 800), vec![(0, 1, 1)])]);
    }

    #[test]
    fn release_all_lifts_and_lets_go() {
        let (mut p, rec) = pad();
        p.handle(0, frame(1, vec![(0, 1, 1)]), false);
        p.handle(
            0,
            PadMsg::Hold {
                mods: vec![Mod::Super],
            },
            false,
        );
        p.release_all();
        assert_eq!(
            rec.calls()[2..],
            [Call::Touch((390, 800), vec![]), Call::Release]
        );
    }

    #[test]
    fn text_is_queued_for_the_caller() {
        let (mut p, _) = pad();
        p.handle(
            0,
            PadMsg::Text {
                text: "Hello, café.".into(),
            },
            false,
        );
        assert_eq!(p.take_text().as_deref(), Some("Hello, café."));
        assert_eq!(p.take_text(), None);
    }
}
