//! Playing touch frames out on a steady cadence, as a real touchpad reports.
//!
//! libinput reads pinch scale, scroll speed and pointer acceleration from the time between
//! kernel events, and uinput stamps each event when it is written. Frames written the moment
//! they land would carry network jitter straight into gesture speed, so each frame is instead
//! placed on the phone's own timeline (its `t`, unwrapped across u32 wrap) and [`Pacer::tick`],
//! called every [`period`], renders the phone's position at one moment of that timeline:
//! `T = now − offset − delay`. `offset` is the smallest arrival − client time seen over
//! [`WINDOW_MS`]; `delay` is how far behind the phone to play (`delay.rs`), measured from the
//! frames that moved.
//!
//! At `T`, two frames holding the same fingers are interpolated between; across a finger landing
//! or lifting the earlier frame is held until `T` passes the later one, and past the newest frame
//! it is held — never extrapolated, since overshoot reads as a jump. Every landing and lifting is
//! played even when several fall between two ticks, so a quick tap still clicks; plain moves
//! between ticks collapse into the rendered position.
//!
//! A tick that holds the newest frame while the finger turns out to have been moving — the next
//! frame had not landed in time — is counted as starved: the measure of a delay set too short.

use std::collections::VecDeque;
use std::time::Duration;

use wado_protocol::pad::Contact;

use super::delay::Delay;

/// How far back the best arrival − client time is remembered.
pub const WINDOW_MS: u64 = 2000;

/// How often [`Pacer::tick`] runs for `trackpad { rate-hz }`, clamped to 60..=250 Hz.
pub fn period(rate_hz: u32) -> Duration {
    Duration::from_micros(1_000_000 / u64::from(rate_hz.clamp(60, 250)))
}

pub struct Pacer {
    delay: Delay,
    /// Which path the timeline belongs to: a switch starts a new one.
    relayed: bool,
    /// The newest frame `T` has passed, on the client timeline.
    base: Option<(i64, Vec<Contact>)>,
    /// Frames `T` has not reached yet, oldest first.
    queue: VecDeque<(i64, Vec<Contact>)>,
    /// (arrival ms, arrival − unwrapped client time), oldest first.
    offsets: VecDeque<(u64, i64)>,
    /// The last client time seen, raw and unwrapped across u32 wrap.
    last_t: Option<(u32, i64)>,
    /// What the sink was last given by this pacer; `None` when unknown.
    sent: Option<Vec<Contact>>,
    /// The last frame pushed that differed from the one before it (repeats and the resting
    /// finger's resends are not news), on the client timeline.
    prev: Option<(i64, Vec<Contact>)>,
    /// Ticks since the last push that held the newest frame with a finger down.
    holding: u32,
    starved: u32,
}

impl Pacer {
    pub fn new(delay: Delay) -> Pacer {
        Pacer {
            delay,
            relayed: false,
            base: None,
            queue: VecDeque::new(),
            offsets: VecDeque::new(),
            last_t: None,
            sent: None,
            prev: None,
            holding: 0,
            starved: 0,
        }
    }

    /// Queue a frame stamped `t` (client ms, wrapping) that arrived at `now_ms` by either path.
    /// True when it is already late: the playhead has passed it.
    pub fn push(&mut self, now_ms: u64, t: u32, c: Vec<Contact>, relayed: bool) -> bool {
        let (t, late) = self.place(now_ms, t, relayed);
        let moved = self
            .prev
            .as_ref()
            .filter(|(_, p)| !c.is_empty() && same_fingers(p, &c) && *p != c)
            .map(|&(pt, _)| t - pt);
        // The ticks that held the last frame were a finger moving, not resting.
        let holding = std::mem::take(&mut self.holding);
        if let Some(interval) = moved {
            self.starved += holding;
            let lateness = now_ms as i64 - t - self.best_offset().unwrap_or(0);
            self.delay.record(interval, lateness);
        }
        if self.prev.as_ref().is_none_or(|(_, p)| *p != c) {
            self.prev = Some((t, c.clone()));
        }
        self.queue.push_back((t, c));
        late
    }

    /// The delay in use on the current path, in ms.
    pub fn delay_ms(&self) -> f64 {
        self.delay.ms(self.relayed)
    }

    /// Ticks starved since the last call.
    pub fn take_starved(&mut self) -> u32 {
        std::mem::take(&mut self.starved)
    }

    /// A direct frame written as it arrived (smoothing off): it moves the timeline along, for
    /// the lateness count, and nothing queued may play after it. True when it would have been
    /// late.
    pub fn bypass(&mut self, now_ms: u64, t: u32) -> bool {
        let (_, late) = self.place(now_ms, t, false);
        self.clear();
        late
    }

    /// The frames to write now, in order.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Vec<Contact>> {
        let mut out = Vec::new();
        self.delay.tick();
        let Some(play) = self.playhead(now_ms) else {
            return out;
        };
        while self.queue.front().is_some_and(|&(t, _)| t as f64 <= play) {
            let next = self.queue.pop_front().unwrap();
            // A finger lands or lifts: the run before it ends where it ended, played exactly.
            if let Some((_, b)) = self.base.take_if(|(_, b)| !same_fingers(b, &next.1)) {
                self.emit(b, &mut out);
            }
            self.base = Some(next);
        }
        if let Some((bt, b)) = &self.base {
            let frame = match self.queue.front() {
                Some((nt, n)) if same_fingers(b, n) => {
                    lerp(b, n, (play - *bt as f64) / (nt - bt).max(1) as f64)
                }
                _ => b.clone(),
            };
            if self.queue.is_empty() && !b.is_empty() && play > *bt as f64 {
                self.holding += 1;
            }
            self.emit(frame, &mut out);
        }
        out
    }

    /// Nothing to play and nothing to settle: a tick now would write nothing and change
    /// nothing — no frame queued, the last one (no fingers) already written, no starved ticks
    /// to report, the delay where it is headed.
    pub fn idle(&self) -> bool {
        self.queue.is_empty()
            && self
                .base
                .as_ref()
                .is_none_or(|(_, b)| b.is_empty() && self.sent.as_ref() == Some(b))
            && self.starved == 0
            && self.delay.settled()
    }

    /// Forget every frame (the pad lifted its fingers, or resized); the sink's state is no
    /// longer known.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.base = None;
        self.sent = None;
        self.prev = None;
        self.holding = 0;
    }

    fn best_offset(&self) -> Option<i64> {
        self.offsets.iter().map(|&(_, o)| o).min()
    }

    /// `T`: the client time to render at `now_ms`; `None` before any frame.
    fn playhead(&self, now_ms: u64) -> Option<f64> {
        Some((now_ms as i64 - self.best_offset()?) as f64 - self.delay_ms())
    }

    /// Put a frame on the client timeline: (its unwrapped time, whether it is already late).
    fn place(&mut self, now_ms: u64, t: u32, relayed: bool) -> (i64, bool) {
        if relayed != self.relayed {
            // The other path has its own latency: the old offset would misplace every frame.
            self.relayed = relayed;
            self.retime();
        }
        let t = match self.last_t {
            Some((raw, unwrapped)) => {
                let step = t.wrapping_sub(raw) as i32 as i64;
                if step < 0 {
                    // The phone's clock went back (a reloaded page): a new timeline.
                    self.retime();
                    t as i64
                } else {
                    unwrapped + step
                }
            }
            None => t as i64,
        };
        self.last_t = Some((t as u32, t));
        while self
            .offsets
            .front()
            .is_some_and(|&(at, _)| at + WINDOW_MS < now_ms)
        {
            self.offsets.pop_front();
        }
        self.offsets.push_back((now_ms, now_ms as i64 - t));
        let late = self.playhead(now_ms).is_some_and(|play| (t as f64) < play);
        (t, late)
    }

    /// Start a new timeline: what was queued on the old one cannot be placed on it.
    /// The measured delay goes too: the new path's frames arrive their own way.
    fn retime(&mut self) {
        self.queue.clear();
        self.base = None;
        self.offsets.clear();
        self.last_t = None;
        self.prev = None;
        self.holding = 0;
        self.delay.reset();
    }

    fn emit(&mut self, frame: Vec<Contact>, out: &mut Vec<Vec<Contact>>) {
        if self.sent.as_ref() != Some(&frame) {
            self.sent = Some(frame.clone());
            out.push(frame);
        }
    }
}

fn same_fingers(a: &[Contact], b: &[Contact]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.iter().any(|y| y.0 == x.0))
}

/// `a` moved `f` (0..=1) of the way to `b`, finger by finger.
fn lerp(a: &[Contact], b: &[Contact], f: f64) -> Vec<Contact> {
    let f = f.clamp(0.0, 1.0);
    let mix = |p: u16, q: u16| (p as f64 + (q as f64 - p as f64) * f).round() as u16;
    a.iter()
        .map(|&(id, x, y)| match b.iter().find(|c| c.0 == id) {
            Some(&(_, bx, by)) => (id, mix(x, bx), mix(y, by)),
            None => (id, x, y),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xs(frames: &[Vec<Contact>]) -> Vec<u16> {
        frames.iter().map(|f| f[0].1).collect()
    }

    #[test]
    fn period_follows_rate_within_bounds() {
        assert_eq!(period(125), Duration::from_millis(8));
        assert_eq!(period(1000), Duration::from_millis(4));
        assert_eq!(period(0), Duration::from_micros(16_666));
    }

    #[test]
    fn the_relay_plays_at_least_its_own_delay_behind() {
        let mut p = fixed(8);
        p.push(1000, 0, vec![(0, 1, 1)], true);
        assert!(p.tick(1023).is_empty());
        assert_eq!(p.tick(1024), vec![vec![(0, 1, 1)]]);
        let mut p = fixed(40);
        p.push(1000, 0, vec![(0, 1, 1)], true);
        assert!(p.tick(1039).is_empty(), "a longer direct delay holds too");
        assert_eq!(p.tick(1040).len(), 1);
        let mut p = fixed(500);
        p.push(1000, 0, vec![(0, 1, 1)], false);
        assert!(p.tick(1049).is_empty(), "clamped to 50");
        assert_eq!(p.tick(1050).len(), 1);
    }

    #[test]
    fn same_fingers_are_interpolated() {
        let mut p = fixed(16);
        p.push(1000, 0, vec![(0, 0, 100)], false);
        p.push(1016, 16, vec![(0, 160, 300)], false);
        assert_eq!(p.tick(1016), vec![vec![(0, 0, 100)]]);
        assert_eq!(p.tick(1024), vec![vec![(0, 80, 200)]]);
        assert_eq!(p.tick(1028), vec![vec![(0, 120, 250)]]);
    }

    #[test]
    fn a_finger_landing_holds_the_earlier_frame() {
        let mut p = fixed(16);
        p.push(1000, 0, vec![(0, 0, 0)], false);
        p.push(1016, 16, vec![(0, 160, 0), (1, 500, 500)], false);
        assert_eq!(p.tick(1016), vec![vec![(0, 0, 0)]]);
        assert!(p.tick(1024).is_empty(), "held, so nothing new");
        assert_eq!(p.tick(1032), vec![vec![(0, 160, 0), (1, 500, 500)]]);
    }

    #[test]
    fn never_extrapolates_past_the_newest_frame() {
        let mut p = fixed(0);
        p.push(1000, 0, vec![(0, 0, 0)], false);
        p.push(1016, 16, vec![(0, 160, 0)], false);
        assert_eq!(xs(&p.tick(1016)), vec![160]);
        assert!(p.tick(1024).is_empty());
        assert!(
            p.tick(1100).is_empty(),
            "the finger rests where it was last seen"
        );
    }

    #[test]
    fn a_tap_between_two_ticks_still_clicks() {
        let mut p = fixed(8);
        p.push(1000, 0, vec![], false);
        assert_eq!(p.tick(1008), vec![vec![]]);
        p.push(1010, 10, vec![(0, 5, 5)], false);
        p.push(1014, 14, vec![], false);
        assert_eq!(p.tick(1024), vec![vec![(0, 5, 5)], vec![]]);
    }

    #[test]
    fn burst_collapses_but_keeps_taps() {
        let mut p = fixed(8);
        p.push(1000, 0, vec![(0, 1, 1)], true); // baseline offset 1000
        // a 300 ms stall, then everything at once
        let burst = [
            vec![(0, 2, 2)],
            vec![(0, 3, 3)],
            vec![],
            vec![(1, 9, 9)],
            vec![],
        ];
        for (i, f) in burst.into_iter().enumerate() {
            assert!(p.push(1316, 16 * (i as u32 + 1), f, true), "late");
        }
        // the moves collapse into the last one before the lift; the tap (down, lift) survives
        assert_eq!(
            p.tick(2000),
            vec![vec![(0, 3, 3)], vec![], vec![(1, 9, 9)], vec![]]
        );
    }

    #[test]
    fn client_clock_wrap_does_not_stall_playout() {
        let mut p = fixed(8);
        p.push(1000, u32::MAX - 7, vec![(0, 1, 1)], true);
        p.push(1016, 8, vec![(0, 2, 2)], true); // 16 ms later on the client, across the wrap
        assert_eq!(p.tick(1024), vec![vec![(0, 1, 1)]]);
        assert_eq!(p.tick(1040), vec![vec![(0, 2, 2)]]);
    }

    #[test]
    fn a_clock_that_goes_back_starts_a_new_timeline() {
        let mut p = fixed(8);
        p.push(1000, 500_000, vec![], false);
        assert_eq!(p.tick(1008), vec![vec![]]);
        p.push(2000, 3, vec![(0, 7, 7)], false); // a reloaded page counts from zero
        assert!(p.tick(2007).is_empty());
        assert_eq!(p.tick(2008), vec![vec![(0, 7, 7)]]);
    }

    #[test]
    fn switching_path_drops_the_old_queue() {
        let mut p = fixed(8);
        p.push(1000, 0, vec![(0, 1, 1)], true);
        p.push(1005, 32, vec![], false); // the direct link caught up
        assert_eq!(p.tick(2000), vec![vec![]]);
    }

    fn fixed(ms: u64) -> Pacer {
        Pacer::new(Delay::fixed(ms))
    }

    struct Run {
        /// x written at each tick, or `None` when the tick wrote nothing.
        xs: Vec<Option<u16>>,
        /// The delay in use at each tick.
        delays: Vec<f64>,
        starved: u32,
    }

    /// A finger drawing a straight line, 10 units per ms, one frame every `interval` ms, each
    /// arriving 0..=`jitter` ms late at random (the first on time), for `ms`. Ticks every 8 ms,
    /// 4 ms out of step with the frames. An overtaken frame is dropped, as the pad drops it.
    fn run(p: &mut Pacer, interval: u32, jitter: u32, ms: u32) -> Run {
        let mut seed = 0x2545_f491u32;
        let mut late = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            u64::from((seed >> 16) % (jitter + 1))
        };
        let mut sent: Vec<(u64, u32, u16)> = (0..ms / interval)
            .map(|k| {
                let j = if k == 0 { 0 } else { late() };
                let t = interval * k;
                (1000 + u64::from(t) + j, t, (1000 + 10 * t) as u16)
            })
            .collect();
        sent.sort_by_key(|s| s.0);
        let (mut i, mut newest) = (0, None);
        let mut r = Run {
            xs: Vec::new(),
            delays: Vec::new(),
            starved: 0,
        };
        for now in (1004..1000 + u64::from(ms)).step_by(8) {
            while i < sent.len() && sent[i].0 <= now {
                let (at, t, x) = sent[i];
                if newest.is_none_or(|n| t > n) {
                    newest = Some(t);
                    p.push(at, t, vec![(0, x, 0)], false);
                }
                i += 1;
            }
            r.xs.push(xs(&p.tick(now)).last().copied());
            r.delays.push(p.delay_ms());
        }
        r.starved = p.take_starved();
        r
    }

    /// Every tick of `xs` wrote a frame, and each moved the same amount, ±1 unit.
    fn assert_even(xs: &[Option<u16>]) {
        let xs: Vec<u16> = xs.iter().map(|x| x.expect("a frame every tick")).collect();
        let steps: Vec<i32> = xs.windows(2).map(|w| w[1] as i32 - w[0] as i32).collect();
        let mean = steps.iter().sum::<i32>() as f64 / steps.len() as f64;
        for s in &steps {
            assert!(
                *s > 0 && (*s as f64 - mean).abs() <= 1.0,
                "step {s} vs mean {mean}: {steps:?}"
            );
        }
    }

    /// 60 Hz frames 0–20 ms late come out even once the delay covers interval + lateness.
    #[test]
    fn jittered_motion_comes_out_even() {
        let r = run(&mut fixed(40), 16, 20, 1600);
        assert_even(&r.xs[6..]);
    }

    #[test]
    fn adaptive_settles_on_a_60hz_phone_and_plays_it_evenly() {
        let mut p = Pacer::new(Delay::adaptive(8, 40));
        let r = run(&mut p, 16, 8, 3000);
        let settled = *r.delays.last().unwrap();
        assert!((16.0..=30.0).contains(&settled), "{settled}");
        assert_even(&r.xs[r.xs.len() - 125..]);
        assert_eq!(r.starved, 0);
    }

    #[test]
    fn adaptive_plays_a_120hz_phone_closer_and_moves_slowly() {
        let mut p = Pacer::new(Delay::adaptive(8, 40));
        let r = run(&mut p, 8, 2, 3000);
        let settled = *r.delays.last().unwrap();
        assert!(settled < 16.0, "{settled}");
        for w in r.delays.windows(2) {
            assert!((w[1] - w[0]).abs() <= 1.0, "{} → {}", w[0], w[1]);
        }
        assert_even(&r.xs[r.xs.len() - 125..]);
    }

    #[test]
    fn starved_ticks_are_counted() {
        assert_eq!(run(&mut fixed(40), 16, 8, 2000).starved, 0);
        assert!(run(&mut fixed(8), 16, 0, 2000).starved > 0);
    }

    #[test]
    fn a_resting_finger_is_not_starved() {
        let mut p = fixed(8);
        p.push(1000, 0, vec![(0, 5, 5)], false);
        for now in (1008..1100).step_by(8) {
            p.tick(now);
        }
        p.push(1100, 100, vec![(0, 5, 5)], false); // the 100 ms resend: it did not move
        p.push(1116, 116, vec![(0, 9, 9)], false);
        assert_eq!(p.take_starved(), 0);
    }
}
