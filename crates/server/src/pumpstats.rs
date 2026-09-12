//! Distribution of how long the WebRTC pump takes per frame.
//!
//! **Why percentiles and not a threshold.** The pump previously logged only overruns past
//! 100 ms. That hides the shape of the thing being measured: a pump at p50 = 2 ms with a rare
//! 400 ms spike and a pump at p50 = 90 ms produce the same handful of warnings, and there is no
//! way to tell them apart from the log. `memory/latency/07` records a "pattern" that was read
//! off exactly that censored view and did not survive contact with the full distribution.
//!
//! Every frame is recorded; one line is emitted per stretch. The outlier warning stays — an
//! individual 400 ms stall is still worth its own line, with the causes attached — but it is no
//! longer the only thing anyone can see.

use std::time::{Duration, Instant};

/// Frames per reported stretch. 300 is five seconds at 60 fps and 2.5 at 120 — short enough to
/// localise a bad patch, long enough that p99 means something (three frames, not a third of one).
const STRETCH: usize = 300;

/// One stretch's worth of per-frame pump timings, in milliseconds.
pub struct PumpStats {
    samples: Vec<f64>,
    budget_overruns: u64,
    started: Instant,
}

/// What [`PumpStats::due`] hands back when a stretch completes.
pub struct Summary {
    pub n: usize,
    pub p50: f64,
    pub p90: f64,
    pub p99: f64,
    pub max: f64,
    pub over_budget: u64,
}

impl PumpStats {
    pub fn new() -> Self {
        Self { samples: Vec::with_capacity(STRETCH), budget_overruns: 0, started: Instant::now() }
    }

    /// Record one frame's pump time.
    pub fn record(&mut self, took: Duration) {
        self.samples.push(took.as_secs_f64() * 1000.0);
    }

    /// Count a frame that missed its budget, for the ratio in the summary.
    pub fn record_over_budget(&mut self) {
        self.budget_overruns += 1;
    }

    /// `Some` once a stretch is complete, and resets. `None` the rest of the time.
    pub fn due(&mut self) -> Option<Summary> {
        if self.samples.len() < STRETCH {
            return None;
        }
        // Sorted copy rather than a streaming estimator: 300 f64s is nothing next to encoding a
        // frame, and an exact percentile cannot be argued with later.
        let mut v = std::mem::take(&mut self.samples);
        v.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
        let s = Summary {
            n: v.len(),
            p50: at(0.50),
            p90: at(0.90),
            p99: at(0.99),
            max: *v.last().unwrap_or(&0.0),
            over_budget: self.budget_overruns,
        };
        self.samples = Vec::with_capacity(STRETCH);
        self.budget_overruns = 0;
        self.started = Instant::now();
        Some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_once_per_stretch_with_exact_percentiles() {
        let mut p = PumpStats::new();
        // 1..=300 ms, so every percentile is a known value.
        for i in 1..=STRETCH {
            assert!(p.due().is_none(), "reported early at {i}");
            p.record(Duration::from_millis(i as u64));
        }
        let s = p.due().expect("a full stretch should report");
        assert_eq!(s.n, STRETCH);
        // Nearest-rank on 300 samples: index round(299 * 0.5) = 150, i.e. the 151st value.
        assert_eq!(s.p50, 151.0);
        assert_eq!(s.p90, 270.0);
        assert_eq!(s.p99, 297.0);
        assert_eq!(s.max, 300.0);
        // Reset: the next stretch starts empty, so one bad patch cannot colour every later line.
        assert!(p.due().is_none());
    }

    #[test]
    fn a_single_spike_does_not_move_the_median() {
        let mut p = PumpStats::new();
        for _ in 0..STRETCH - 1 {
            p.record(Duration::from_millis(2));
        }
        p.record(Duration::from_millis(400));
        let s = p.due().unwrap();
        assert_eq!(s.p50, 2.0, "the median is what the threshold log could never show");
        assert_eq!(s.max, 400.0, "and the outlier is still visible");
    }
}
