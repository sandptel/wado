//! Playing touch frames out on a steady cadence, as a real touchpad reports.
//!
//! libinput reads pinch scale, scroll speed and pointer acceleration from the time between
//! kernel events, and uinput stamps each event when it is written. Frames written the moment
//! they land would carry network jitter straight into gesture speed, so each frame is instead
//! placed on the phone's own timeline (its `t`, unwrapped across u32 wrap) and [`Pacer::tick`],
//! called every [`period`], renders the phone's position at one moment of that timeline:
//! `T = now − offset − delay`. `offset` is the smallest arrival − client time seen over
//! [`WINDOW_MS`]; `delay` is `trackpad { direct-delay-ms }`, or at least [`RELAY_DELAY_MS`] on the
//! relay path, whose TCP stalls deliver frames in bursts.
//!
//! At `T`, two frames holding the same fingers are interpolated between; across a finger landing
//! or lifting the earlier frame is held until `T` passes the later one, and past the newest frame
//! it is held — never extrapolated, since overshoot reads as a jump. Every landing and lifting is
//! played even when several fall between two ticks, so a quick tap still clicks; plain moves
//! between ticks collapse into the rendered position.

use std::collections::VecDeque;
use std::time::Duration;

use wado_protocol::pad::Contact;

/// The least delay on the relay path, whose stalls deliver frames in bursts.
pub const RELAY_DELAY_MS: u64 = 24;
/// How far back the best arrival − client time is remembered.
pub const WINDOW_MS: u64 = 2000;

/// How often [`Pacer::tick`] runs for `trackpad { rate-hz }`, clamped to 60..=250 Hz.
pub fn period(rate_hz: u32) -> Duration {
    Duration::from_micros(1_000_000 / u64::from(rate_hz.clamp(60, 250)))
}

pub struct Pacer {
    direct_delay_ms: u64,
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
}

impl Pacer {
    /// `direct_delay_ms` is `trackpad { direct-delay-ms }`, clamped to 0..=50.
    pub fn new(direct_delay_ms: u64) -> Pacer {
        Pacer {
            direct_delay_ms: direct_delay_ms.min(50),
            relayed: false,
            base: None,
            queue: VecDeque::new(),
            offsets: VecDeque::new(),
            last_t: None,
            sent: None,
        }
    }

    /// Queue a frame stamped `t` (client ms, wrapping) that arrived at `now_ms` by either path.
    /// True when it is already late: the playhead has passed it.
    pub fn push(&mut self, now_ms: u64, t: u32, c: Vec<Contact>, relayed: bool) -> bool {
        let (t, late) = self.place(now_ms, t, relayed);
        self.queue.push_back((t, c));
        late
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
        let Some(play) = self.playhead(now_ms) else {
            return out;
        };
        while self.queue.front().is_some_and(|&(t, _)| t <= play) {
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
                    lerp(b, n, (play - bt) as f64 / (nt - bt).max(1) as f64)
                }
                _ => b.clone(),
            };
            self.emit(frame, &mut out);
        }
        out
    }

    /// Forget every frame (the pad lifted its fingers, or resized); the sink's state is no
    /// longer known.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.base = None;
        self.sent = None;
    }

    fn delay(&self) -> u64 {
        if self.relayed {
            self.direct_delay_ms.max(RELAY_DELAY_MS)
        } else {
            self.direct_delay_ms
        }
    }

    /// `T`: the client time to render at `now_ms`; `None` before any frame.
    fn playhead(&self, now_ms: u64) -> Option<i64> {
        let offset = self.offsets.iter().map(|&(_, o)| o).min()?;
        Some(now_ms as i64 - offset - self.delay() as i64)
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
        let late = self.playhead(now_ms).is_some_and(|play| t < play);
        (t, late)
    }

    /// Start a new timeline: what was queued on the old one cannot be placed on it.
    fn retime(&mut self) {
        self.queue.clear();
        self.base = None;
        self.offsets.clear();
        self.last_t = None;
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
        let mut p = Pacer::new(8);
        p.push(1000, 0, vec![(0, 1, 1)], true);
        assert!(p.tick(1023).is_empty());
        assert_eq!(p.tick(1024), vec![vec![(0, 1, 1)]]);
        let mut p = Pacer::new(40);
        p.push(1000, 0, vec![(0, 1, 1)], true);
        assert!(p.tick(1039).is_empty(), "a longer direct delay holds too");
        assert_eq!(p.tick(1040).len(), 1);
        let mut p = Pacer::new(500);
        p.push(1000, 0, vec![(0, 1, 1)], false);
        assert!(p.tick(1049).is_empty(), "clamped to 50");
        assert_eq!(p.tick(1050).len(), 1);
    }

    #[test]
    fn same_fingers_are_interpolated() {
        let mut p = Pacer::new(16);
        p.push(1000, 0, vec![(0, 0, 100)], false);
        p.push(1016, 16, vec![(0, 160, 300)], false);
        assert_eq!(p.tick(1016), vec![vec![(0, 0, 100)]]);
        assert_eq!(p.tick(1024), vec![vec![(0, 80, 200)]]);
        assert_eq!(p.tick(1028), vec![vec![(0, 120, 250)]]);
    }

    #[test]
    fn a_finger_landing_holds_the_earlier_frame() {
        let mut p = Pacer::new(16);
        p.push(1000, 0, vec![(0, 0, 0)], false);
        p.push(1016, 16, vec![(0, 160, 0), (1, 500, 500)], false);
        assert_eq!(p.tick(1016), vec![vec![(0, 0, 0)]]);
        assert!(p.tick(1024).is_empty(), "held, so nothing new");
        assert_eq!(p.tick(1032), vec![vec![(0, 160, 0), (1, 500, 500)]]);
    }

    #[test]
    fn never_extrapolates_past_the_newest_frame() {
        let mut p = Pacer::new(0);
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
        let mut p = Pacer::new(8);
        p.push(1000, 0, vec![], false);
        assert_eq!(p.tick(1008), vec![vec![]]);
        p.push(1010, 10, vec![(0, 5, 5)], false);
        p.push(1014, 14, vec![], false);
        assert_eq!(p.tick(1024), vec![vec![(0, 5, 5)], vec![]]);
    }

    #[test]
    fn burst_collapses_but_keeps_taps() {
        let mut p = Pacer::new(8);
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
        let mut p = Pacer::new(8);
        p.push(1000, u32::MAX - 7, vec![(0, 1, 1)], true);
        p.push(1016, 8, vec![(0, 2, 2)], true); // 16 ms later on the client, across the wrap
        assert_eq!(p.tick(1024), vec![vec![(0, 1, 1)]]);
        assert_eq!(p.tick(1040), vec![vec![(0, 2, 2)]]);
    }

    #[test]
    fn a_clock_that_goes_back_starts_a_new_timeline() {
        let mut p = Pacer::new(8);
        p.push(1000, 500_000, vec![], false);
        assert_eq!(p.tick(1008), vec![vec![]]);
        p.push(2000, 3, vec![(0, 7, 7)], false); // a reloaded page counts from zero
        assert!(p.tick(2007).is_empty());
        assert_eq!(p.tick(2008), vec![vec![(0, 7, 7)]]);
    }

    #[test]
    fn switching_path_drops_the_old_queue() {
        let mut p = Pacer::new(8);
        p.push(1000, 0, vec![(0, 1, 1)], true);
        p.push(1005, 32, vec![], false); // the direct link caught up
        assert_eq!(p.tick(2000), vec![vec![]]);
    }

    /// A steady 16 ms-spaced line, arriving 0–20 ms late at random, comes out evenly spaced at
    /// every 8 ms tick once the delay covers a frame interval plus the jitter (16 + 20 ≤ 40).
    #[test]
    fn jittered_motion_comes_out_even() {
        let mut seed = 0x2545_f491u32;
        let mut jitter = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            u64::from((seed >> 16) % 21)
        };
        // (arrival, client t, x); the first arrives on time, so the offset is primed at once
        let mut sent: Vec<(u64, u32, u16)> = (0..100u32)
            .map(|k| {
                let j = if k == 0 { 0 } else { jitter() };
                (
                    1000 + u64::from(16 * k) + j,
                    16 * k,
                    (1000 + 160 * k) as u16,
                )
            })
            .collect();
        sent.sort_by_key(|s| s.0);
        let mut p = Pacer::new(40);
        let (mut i, mut newest) = (0, None);
        let mut out = Vec::new();
        for now in (1000..2600).step_by(8) {
            while i < sent.len() && sent[i].0 <= now {
                let (at, t, x) = sent[i];
                if newest.is_none_or(|n| t > n) {
                    // an overtaken frame is stale, as the pad drops it
                    newest = Some(t);
                    p.push(at, t, vec![(0, x, 0)], false);
                }
                i += 1;
            }
            out.extend(xs(&p.tick(now)));
        }
        let steps: Vec<i32> = out.windows(2).map(|w| w[1] as i32 - w[0] as i32).collect();
        assert!(steps.len() > 150, "a frame every tick: {}", steps.len());
        let mean = steps.iter().sum::<i32>() as f64 / steps.len() as f64;
        for s in &steps {
            assert!(
                *s > 0 && (*s as f64 - mean).abs() <= 1.0,
                "step {s} vs mean {mean}: {steps:?}"
            );
        }
    }
}
