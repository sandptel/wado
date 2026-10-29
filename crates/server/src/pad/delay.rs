//! How far behind the phone the pacer plays.
//!
//! The pacer interpolates between two frames, so the next frame must have landed by the time
//! the playhead passes the current one: the delay has to cover the phone's frame interval plus
//! how late its frames run. `trackpad { delay "adaptive" }` measures both from frames that
//! moved — the median interval between them on the phone's clock, plus the 98th percentile of
//! how far each arrived behind the best recent arrival — over the last [`WINDOW`] such frames,
//! within `direct-delay-min..=direct-delay-max`. Gesture speed reads from the playhead's pace, so
//! the delay in use only drifts toward that estimate, [`SLEW_MS`] per tick. `delay "fixed"`
//! plays `direct-delay-ms` behind. The relay path plays at least [`RELAY_DELAY_MS`] behind.

use std::collections::VecDeque;

use wado_config::schema::Trackpad;
use wado_config::schema::trackpad::TrackpadDelay;

/// The least delay on the relay path, whose stalls deliver frames in bursts.
pub const RELAY_DELAY_MS: u64 = 24;
/// How many moving frames the estimate is taken over (about 2 s of motion at 60 Hz).
pub const WINDOW: usize = 120;
/// Fewer moving frames than this and the estimate is not trusted yet.
pub const MIN_SAMPLES: usize = 10;
/// Frames further apart than this on the phone's clock were a pause, not the frame interval.
pub const PAUSE_MS: i64 = 100;
/// The most the delay in use moves in one tick: 0.25 ms in an 8 ms tick bends gesture speed
/// by 3% while it settles.
pub const SLEW_MS: f64 = 0.25;
/// The delay before there is anything to measure.
const UNMEASURED_MS: u64 = 24;
/// The quantile of lateness covered: a frame later than this starves the pacer for a moment.
const LATE_QUANTILE: usize = 98;
const CLAMP_MS: u64 = 50;

pub struct Delay {
    adaptive: bool,
    min: u64,
    max: u64,
    /// (interval ms, lateness ms) of recent moving frames, oldest first.
    samples: VecDeque<(i64, i64)>,
    target: f64,
    applied: f64,
}

impl Delay {
    pub fn from_config(cfg: &Trackpad) -> Delay {
        match cfg.delay {
            TrackpadDelay::Adaptive => Delay::adaptive(cfg.direct_delay_min, cfg.direct_delay_max),
            TrackpadDelay::Fixed => Delay::fixed(cfg.direct_delay_ms),
        }
    }

    /// Always `ms` (clamped 0..=50) behind.
    pub fn fixed(ms: u64) -> Delay {
        let ms = ms.min(CLAMP_MS);
        Delay::new(false, ms, ms)
    }

    /// Measured, within `min..=max` (each clamped 0..=50, `max` at least `min`).
    pub fn adaptive(min: u64, max: u64) -> Delay {
        let min = min.min(CLAMP_MS);
        Delay::new(true, min, max.clamp(min, CLAMP_MS))
    }

    fn new(adaptive: bool, min: u64, max: u64) -> Delay {
        let mut d = Delay {
            adaptive,
            min,
            max,
            samples: VecDeque::new(),
            target: 0.0,
            applied: 0.0,
        };
        d.reset();
        d
    }

    /// A frame that moved arrived: `interval` ms after the previous one on the phone's clock,
    /// `lateness` ms behind the best recent arrival.
    pub fn record(&mut self, interval: i64, lateness: i64) {
        if !self.adaptive || interval <= 0 || interval >= PAUSE_MS {
            return;
        }
        if self.samples.len() == WINDOW {
            self.samples.pop_front();
        }
        self.samples.push_back((interval, lateness.max(0)));
        if self.samples.len() >= MIN_SAMPLES {
            let mut intervals: Vec<i64> = self.samples.iter().map(|s| s.0).collect();
            let mut late: Vec<i64> = self.samples.iter().map(|s| s.1).collect();
            let estimate = quantile(&mut intervals, 50) + quantile(&mut late, LATE_QUANTILE);
            self.target = (estimate.max(0) as u64).clamp(self.min, self.max) as f64;
        }
    }

    /// One pacer tick: the delay in use moves toward the estimate.
    pub fn tick(&mut self) {
        self.applied += (self.target - self.applied).clamp(-SLEW_MS, SLEW_MS);
    }

    /// The delay in use, in ms, on the direct or relay path.
    pub fn ms(&self, relayed: bool) -> f64 {
        if relayed {
            self.applied.max(RELAY_DELAY_MS as f64)
        } else {
            self.applied
        }
    }

    /// Forget the measurements (the frames now come another way).
    pub fn reset(&mut self) {
        self.samples.clear();
        self.target = if self.adaptive {
            UNMEASURED_MS.max(self.min).min(self.max)
        } else {
            self.min
        } as f64;
        self.applied = self.target;
    }
}

/// The `p`th percentile, nearest rank.
fn quantile(v: &mut [i64], p: usize) -> i64 {
    v.sort_unstable();
    v[(v.len() * p).div_ceil(100).max(1) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_stays_put_and_the_relay_keeps_its_floor() {
        let mut d = Delay::fixed(8);
        for _ in 0..50 {
            d.record(16, 30);
            d.tick();
        }
        assert_eq!((d.ms(false), d.ms(true)), (8.0, 24.0));
        assert_eq!(Delay::fixed(500).ms(false), 50.0);
    }

    #[test]
    fn unmeasured_plays_24_behind_within_bounds() {
        let mut d = Delay::adaptive(8, 40);
        for _ in 0..MIN_SAMPLES - 1 {
            d.record(8, 0);
            d.tick();
        }
        assert_eq!(d.ms(false), 24.0);
        assert_eq!(Delay::adaptive(30, 40).ms(false), 30.0);
        assert_eq!(Delay::adaptive(4, 12).ms(false), 12.0);
    }

    #[test]
    fn the_estimate_is_interval_plus_lateness_and_is_approached_slowly() {
        let mut d = Delay::adaptive(0, 50);
        for i in 0..WINDOW {
            d.record(16, (i % 9) as i64); // lateness 0..=8
        }
        assert_eq!(d.target, 24.0);
        let mut d = Delay::adaptive(0, 50);
        for _ in 0..WINDOW {
            d.record(8, 2);
        }
        assert_eq!(d.target, 10.0);
        let mut last = d.ms(false);
        for _ in 0..200 {
            d.tick();
            let now = d.ms(false);
            assert!((now - last).abs() <= SLEW_MS, "{last} → {now}");
            last = now;
        }
        assert_eq!(last, 10.0, "settled");
    }

    #[test]
    fn bounds_hold_and_the_relay_takes_a_larger_estimate() {
        let mut d = Delay::adaptive(8, 40);
        for _ in 0..WINDOW {
            d.record(16, 90);
        }
        assert_eq!(d.target, 40.0);
        for _ in 0..200 {
            d.tick();
        }
        assert_eq!((d.ms(false), d.ms(true)), (40.0, 40.0));
        for _ in 0..WINDOW {
            d.record(1, 0);
        }
        assert_eq!(d.target, 8.0);
    }

    #[test]
    fn pauses_are_not_intervals() {
        let mut d = Delay::adaptive(0, 50);
        for _ in 0..WINDOW {
            d.record(PAUSE_MS, 0);
        }
        assert_eq!(d.samples.len(), 0);
    }
}
