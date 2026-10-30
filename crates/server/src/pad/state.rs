//! One phone's pad: what it holds down, and what each message from it does.
//!
//! Shared by both transports, so a phone that starts on the relay and moves to the direct link
//! mid-drag keeps one continuous touch. Pure apart from the [`Devices`] it drives — the
//! platform's injectors, faked in tests — which is what lets the safety rules — stale frames
//! dropped, a silent finger lifted, a resize lifting every finger first — be tested without a
//! kernel device. Touch frames are written by the
//! [`Pacer`] on the pad's tick, except direct ones with `trackpad { smoothing #false }`, which
//! are written as they arrive.

use tracing::info;
use wado_config::schema::Trackpad;
use wado_platform::input::{InputError, InputRes};
use wado_protocol::pad::{Contact, Mod, PadMsg, newer};

use super::arrivals::Arrivals;
use super::delay::Delay;
use super::keys::{char_code, key_code};
use super::pacer::Pacer;
use super::sink::Devices;
use super::ticker::Pace;

pub struct Pad {
    sink: Devices,
    size: Option<(u16, u16)>,
    last_seq: Option<u32>,
    /// Any finger down, as last sent to the sink.
    down: bool,
    last_frame_ms: u64,
    /// When anything at all last arrived, and whether held keys were let go since.
    last_msg_ms: u64,
    keys_released: bool,
    /// `trackpad { finger-silence-ms }`: a finger down with no frame this long is lifted — the
    /// phone or its link is gone. The phone re-sends every 100 ms while a finger rests.
    finger_silence_ms: u64,
    /// `trackpad { key-silence-ms }`: no message at all this long (the phone pings every 2 s)
    /// and held keys are let go too, so a locked phone never leaves Super down for good.
    key_silence_ms: u64,
    /// `trackpad { smoothing }`: direct frames go through the pacer too.
    smoothing: bool,
    pacer: Pacer,
    arrivals: Arrivals,
    text: Vec<String>,
}

impl Pad {
    /// A pad with the `trackpad { }` config as it is now.
    pub fn new(sink: Devices) -> Pad {
        Pad::with_config(sink, &wado_config::live::current().trackpad)
    }

    pub fn with_config(sink: Devices, cfg: &Trackpad) -> Pad {
        Pad {
            finger_silence_ms: cfg.finger_silence_ms,
            key_silence_ms: cfg.key_silence_ms,
            smoothing: cfg.smoothing,
            pacer: Pacer::new(Delay::from_config(cfg)),
            arrivals: Arrivals::default(),
            sink,
            size: None,
            last_seq: None,
            down: false,
            last_frame_ms: 0,
            last_msg_ms: 0,
            keys_released: false,
            text: Vec::new(),
        }
    }

    /// Apply one message that arrived at `now_ms`; `relayed` when it came through the relay.
    /// Returns the reply, if any.
    pub fn handle(&mut self, now_ms: u64, msg: PadMsg, relayed: bool) -> Option<PadMsg> {
        self.last_msg_ms = now_ms;
        self.keys_released = false;
        let result = match msg {
            PadMsg::Size { w, h } => self.resize((w, h)),
            PadMsg::Frame { seq, t, c } => {
                // A reloaded page starts its count again: after a quiet spell with nothing down,
                // any number is taken as the new start rather than dropped as stale.
                let restarted = !self.down
                    && now_ms.saturating_sub(self.last_frame_ms) > self.finger_silence_ms;
                if self.last_seq.is_some_and(|last| !newer(seq, last)) && !restarted {
                    return None;
                }
                self.last_seq = Some(seq);
                self.last_frame_ms = now_ms;
                let c = first_of_each_id(c);
                let down = !c.is_empty();
                if relayed || self.smoothing {
                    let late = self.pacer.push(now_ms, t, c, relayed);
                    self.arrivals.record(now_ms, down, late);
                    return None; // written on the tick
                }
                // Written now; relayed frames still queued are older than this.
                let late = self.pacer.bypass(now_ms, t);
                self.arrivals.record(now_ms, down, late);
                self.touch(&c)
            }
            PadMsg::Key { key, mods } => match key_code(&key) {
                Some(code) => self.sink.tap(&mods, code),
                None => Err(InputError::Failed(format!("unknown key “{key}”"))),
            },
            PadMsg::Char { ch, mut mods } => match char_code(ch) {
                Some((code, shift)) => {
                    if shift && !mods.contains(&Mod::Shift) {
                        mods.push(Mod::Shift);
                    }
                    self.sink.tap(&mods, code)
                }
                None => Err(InputError::Failed(format!(
                    "“{ch}” has no key on this keyboard layout — type it as text instead"
                ))),
            },
            PadMsg::Hold { mods } => self.sink.hold(&mods),
            PadMsg::Text { text } => {
                self.text.push(text);
                Ok(())
            }
            // A phone that offers compact touch frames is told it may send them (`pad_bin`).
            PadMsg::Ping { t, bin } => return Some(PadMsg::Pong { t, bin }),
            // Daemon → phone only; a phone sending them is ignored. A compact frame is unpacked
            // before it gets here (`super::bin`).
            PadMsg::Pong { .. } | PadMsg::Err { .. } | PadMsg::Bye | PadMsg::Bin { .. } => Ok(()),
            // Actions are answered before a pad exists (`actions.rs`), never here.
            PadMsg::Actions
            | PadMsg::ActionList { .. }
            | PadMsg::Run { .. }
            | PadMsg::Ran { .. } => Ok(()),
        };
        result.err().map(reply)
    }

    /// Text waiting to be typed (typing is async; the caller does it).
    pub fn take_text(&mut self) -> Option<String> {
        (!self.text.is_empty()).then(|| self.text.remove(0))
    }

    /// Write what the pacer renders now, and lift fingers that went silent. Called every
    /// `trackpad { rate-hz }`.
    pub fn tick(&mut self, now_ms: u64) -> Option<PadMsg> {
        let mut err = None;
        for frame in self.pacer.tick(now_ms) {
            if let Err(e) = self.touch(&frame) {
                err.get_or_insert(e);
            }
        }
        self.arrivals.starve(self.pacer.take_starved());
        if let Some(line) = self.arrivals.report(now_ms, self.pacer.delay_ms()) {
            info!("{line}");
        }
        if self.down && now_ms.saturating_sub(self.last_frame_ms) > self.finger_silence_ms {
            self.pacer.clear();
            if let Err(e) = self.touch(&[]) {
                err.get_or_insert(e);
            }
        }
        if !self.keys_released && now_ms.saturating_sub(self.last_msg_ms) > self.key_silence_ms {
            self.keys_released = true;
            self.sink.release_all();
        }
        err.map(reply)
    }

    /// What the tick must do next: keep its cadence while a finger is down or the pacer has
    /// anything to play (or a delay still settling); else rest until a message — or until the
    /// next timed rule falls due: held keys let go at key silence, the arrivals summary. (A
    /// silent finger needs no deadline here: a finger down keeps the cadence.)
    pub fn pace(&self) -> Pace {
        if self.down || !self.pacer.idle() {
            return Pace::Tick;
        }
        let keys = (!self.keys_released).then(|| self.last_msg_ms + self.key_silence_ms + 1);
        Pace::Park(keys.into_iter().chain(self.arrivals.due()).min())
    }

    /// Make the devices now (see [`Devices::ready`]).
    pub fn prepare(&mut self) -> InputRes {
        self.sink.ready(self.size)
    }

    /// Let go of held keys (the direct link that held them dropped); fingers are left to the
    /// silence check, since the phone may carry on over the relay.
    pub fn release_keys(&mut self) {
        self.sink.release_all();
    }

    /// Let go of everything: fingers, held keys.
    pub fn release_all(&mut self) {
        self.pacer.clear();
        if self.down {
            let _ = self.touch(&[]);
        }
        self.sink.release_all();
    }

    fn touch(&mut self, c: &[Contact]) -> InputRes {
        let Some(size) = self.size else {
            return Ok(()); // the phone always sends its size first; nothing to touch yet
        };
        self.down = !c.is_empty();
        self.sink.touch(size, c)
    }

    fn resize(&mut self, size: (u16, u16)) -> InputRes {
        let moved = |a: u16, b: u16| a.abs_diff(b) > 2;
        if let Some(old) = self.size {
            if !moved(old.0, size.0) && !moved(old.1, size.1) {
                return Ok(());
            }
            self.pacer.clear();
            if self.down {
                let _ = self.touch(&[]);
            }
        }
        self.size = Some(size);
        self.sink.ready(Some(size))
    }
}

/// A device error, as the phone is told it: one the computer cannot fix by retrying (no
/// `/dev/uinput`) is fatal, so the phone stops instead of sending sixty more a second.
fn reply(e: InputError) -> PadMsg {
    PadMsg::Err {
        fatal: matches!(e, InputError::Unavailable(_)),
        message: e.to_string(),
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
    use super::*;

    /// The built-in timings, which a test's pad gets.
    const KEYS_SILENCE_MS: u64 = 5000;
    #[test]
    fn timings_come_from_config_defaults() {
        let d = wado_config::schema::Trackpad::default();
        assert_eq!(
            (d.finger_silence_ms, d.key_silence_ms),
            (400, KEYS_SILENCE_MS)
        );
    }

    use wado_platform::fake::input::{InputCall as Call, InputLog};

    /// What reached the fake devices, without their openings.
    #[derive(Clone, Default)]
    struct Rec(InputLog);
    impl Rec {
        fn calls(&self) -> Vec<Call> {
            self.0.io()
        }
        fn devices(&self) -> Devices {
            Devices::new(self.0.touch(), self.0.keys())
        }
    }

    /// A pad that writes direct frames the moment they arrive (`smoothing #false`).
    fn pad() -> (Pad, Rec) {
        pad_with(false)
    }
    /// A pad that plays every frame out on its tick (the default).
    fn smooth_pad() -> (Pad, Rec) {
        pad_with(true)
    }
    fn pad_with(smoothing: bool) -> (Pad, Rec) {
        let rec = Rec::default();
        let cfg = wado_config::schema::Trackpad {
            smoothing,
            ..Default::default()
        };
        let mut p = Pad::with_config(rec.devices(), &cfg);
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
            p.handle(0, PadMsg::Ping { t: 5, bin: false }, false),
            Some(PadMsg::Pong { t: 5, bin: false })
        );
        // Offered compact frames: accepted.
        assert_eq!(
            p.handle(0, PadMsg::Ping { t: 6, bin: true }, false),
            Some(PadMsg::Pong { t: 6, bin: true })
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
        assert!(
            matches!(reply, Some(PadMsg::Err { message, fatal: false }) if message.contains('é'))
        );
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
        assert!(
            matches!(reply, Some(PadMsg::Err { message, fatal: false }) if message.contains("hyper"))
        );
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
        p.tick(1023);
        assert_eq!(rec.calls().len(), 0);
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

    #[test]
    fn a_direct_frame_drops_relayed_ones_still_queued() {
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
        p.handle(1005, frame(2, vec![]), false); // the lift, on the direct link
        p.tick(2000);
        assert_eq!(rec.calls(), vec![Call::Touch((390, 800), vec![])]);
    }

    #[test]
    fn a_restarted_count_is_taken_after_a_quiet_spell() {
        let (mut p, rec) = pad();
        p.handle(0, frame(50, vec![]), false);
        p.handle(10, frame(1, vec![(0, 1, 1)]), false); // too soon: stale
        p.handle(1000, frame(1, vec![(0, 2, 2)]), false); // a reloaded page
        assert_eq!(
            rec.calls().last(),
            Some(&Call::Touch((390, 800), vec![(0, 2, 2)]))
        );
        assert_eq!(rec.calls().len(), 2);
    }

    #[test]
    fn held_keys_go_after_total_silence() {
        let (mut p, rec) = pad();
        p.handle(
            0,
            PadMsg::Hold {
                mods: vec![Mod::Super],
            },
            false,
        );
        p.tick(KEYS_SILENCE_MS);
        assert!(!rec.calls().contains(&Call::Release));
        p.tick(KEYS_SILENCE_MS + 1);
        p.tick(KEYS_SILENCE_MS + 500);
        assert_eq!(
            rec.calls().iter().filter(|c| **c == Call::Release).count(),
            1
        );
    }

    #[test]
    fn smoothed_frames_wait_for_the_tick() {
        let (mut p, rec) = smooth_pad();
        p.handle(1000, frame(1, vec![(0, 1, 1)]), false);
        assert_eq!(rec.calls().len(), 0, "not written on arrival");
        p.tick(1023);
        assert_eq!(rec.calls().len(), 0, "unmeasured, the delay is 24 ms");
        p.tick(1024);
        assert_eq!(rec.calls(), vec![Call::Touch((390, 800), vec![(0, 1, 1)])]);
    }

    #[test]
    fn a_smoothed_tap_between_ticks_still_clicks() {
        let (mut p, rec) = smooth_pad();
        p.handle(1000, frame(1, vec![]), false); // t 16
        p.tick(1008);
        p.handle(1010, frame(2, vec![(0, 5, 5)]), false); // t 32, 2 ms early
        p.handle(1012, frame(3, vec![]), false); // t 48, 20 ms early
        p.tick(1056);
        assert_eq!(
            rec.calls(),
            vec![
                Call::Touch((390, 800), vec![]),
                Call::Touch((390, 800), vec![(0, 5, 5)]),
                Call::Touch((390, 800), vec![]),
            ]
        );
    }

    #[test]
    fn a_smoothed_silent_finger_lifts_once_and_stays_up() {
        let (mut p, rec) = smooth_pad();
        p.handle(1000, frame(1, vec![(0, 1, 1)]), false);
        p.tick(1024);
        p.tick(1401);
        p.tick(1409);
        assert_eq!(
            rec.calls(),
            vec![
                Call::Touch((390, 800), vec![(0, 1, 1)]),
                Call::Touch((390, 800), vec![]),
            ]
        );
    }

    #[test]
    fn a_smoothed_direct_frame_drops_relayed_ones_still_queued() {
        let (mut p, rec) = smooth_pad();
        p.handle(
            1000,
            PadMsg::Frame {
                seq: 1,
                t: 0,
                c: vec![(0, 1, 1)],
            },
            true,
        );
        p.handle(1005, frame(2, vec![]), false);
        p.tick(2000);
        assert_eq!(rec.calls(), vec![Call::Touch((390, 800), vec![])]);
    }

    #[test]
    fn the_delay_comes_from_config() {
        let rec = Rec::default();
        let cfg = wado_config::schema::Trackpad {
            delay: wado_config::schema::trackpad::TrackpadDelay::Fixed,
            direct_delay_ms: 8,
            ..Default::default()
        };
        let mut p = Pad::with_config(rec.devices(), &cfg);
        p.handle(0, PadMsg::Size { w: 390, h: 800 }, false);
        p.handle(1000, frame(1, vec![(0, 1, 1)]), false);
        p.tick(1007);
        assert_eq!(rec.calls().len(), 0);
        p.tick(1008);
        assert_eq!(rec.calls().len(), 1);
    }

    #[test]
    fn a_computer_without_the_devices_says_so_and_it_is_fatal() {
        let rec = Rec::default();
        rec.0
            .refuse(InputError::Unavailable("no uinput here".into()));
        let mut p = Pad::with_config(rec.devices(), &Default::default());
        assert_eq!(
            p.handle(0, PadMsg::Size { w: 390, h: 800 }, false),
            Some(PadMsg::Err {
                message: "no uinput here".into(),
                fatal: true
            })
        );
        assert!(rec.calls().is_empty());
    }
}
