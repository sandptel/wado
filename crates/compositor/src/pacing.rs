//! Render-tick pacing telemetry — one job: watch the *interval* between render ticks
//! and report when the render loop stops hitting its frame budget.
//!
//! The interval matters more than the tick's own duration. A tick that takes 30 ms is
//! merely slow; ticks that arrive with **no idle gap between them** mean the calloop
//! event loop never sleeps, and every other source — remote input, Wayland traffic,
//! control commands — is starved behind the render timer. That starvation is what the
//! user feels as input jitter, so this is the metric that indicts (or clears) the
//! render loop.
//!
//! Self-silencing by design: a healthy loop logs at `debug`, and only a window that
//! actually missed its budget escalates to `warn`. Leaving it armed in release costs
//! one `Instant::now()` per frame.

use std::time::{Duration, Instant};

/// Rolling render-tick interval stats over a fixed window of ticks.
pub struct TickStats {
    /// Ticks per reported window (~2 s worth, so the log is readable, not a firehose).
    window: u32,
    /// The per-frame interval the session is aiming for (1/fps).
    budget: Duration,
    /// Target frame rate, for reporting achieved-vs-asked.
    target_fps: u32,
    last: Option<Instant>,
    last_report: Instant,
    count: u32,
    sum: Duration,
    max: Duration,
}

/// Report at least this often, even if the tick window hasn't filled.
///
/// Windowing on tick *count* alone makes the telemetry quieter the worse things get: at
/// 13.6 fps a 120-tick window takes ~9 s, at 2 fps a minute, and a fully stalled loop
/// never reports at all — silence indistinguishable from "no session". Gating on elapsed
/// time too means a degrading loop gets louder, which is the whole point of the metric.
const REPORT_INTERVAL: Duration = Duration::from_secs(2);

/// How far the mean interval may exceed the budget before we call it a real miss.
///
/// A tick landing at 16.8 ms against a 16.7 ms budget is timer granularity, not
/// starvation — counting those produced a "missed budget" warning on a loop that was
/// holding a steady 60 fps. What actually matters is sustained shortfall, so the gate is
/// the *mean* interval exceeding the budget by this factor (i.e. achieving under ~80% of
/// the requested frame rate).
const MISS_FACTOR: f64 = 1.25;

impl TickStats {
    pub fn new(fps: u32) -> Self {
        let fps = fps.max(1);
        Self {
            window: fps.saturating_mul(2),
            budget: Duration::from_nanos(1_000_000_000 / fps as u64),
            target_fps: fps,
            last: None,
            last_report: Instant::now(),
            count: 0,
            sum: Duration::ZERO,
            max: Duration::ZERO,
        }
    }

    /// Record that a render tick just ran. Emits one summary per window.
    pub fn tick(&mut self) {
        let now = Instant::now();
        // First tick only establishes the baseline — there is no interval yet.
        let Some(prev) = self.last.replace(now) else {
            return;
        };

        let dt = now.saturating_duration_since(prev);
        self.count += 1;
        self.sum += dt;
        if dt > self.max {
            self.max = dt;
        }
        // Whichever comes first: a full window, or REPORT_INTERVAL of wall clock. The
        // `count > 0` guard keeps the time-based path from dividing by zero.
        if self.count >= self.window
            || now.saturating_duration_since(self.last_report) >= REPORT_INTERVAL
        {
            self.report(now);
        }
    }

    fn report(&mut self, now: Instant) {
        self.last_report = now;
        if self.count == 0 {
            return;
        }
        let mean = self.sum / self.count;
        let budget_ms = self.budget.as_secs_f64() * 1e3;
        let mean_ms = mean.as_secs_f64() * 1e3;
        let max_ms = self.max.as_secs_f64() * 1e3;
        // Achieved rate over the window — the number that says whether the loop is
        // actually keeping up, independent of per-tick noise.
        let fps = if mean_ms > 0.0 { 1e3 / mean_ms } else { 0.0 };

        if mean_ms > budget_ms * MISS_FACTOR {
            tracing::warn!(
                fps = format_args!("{fps:.1}"),
                target_fps = self.target_fps,
                mean_ms = format_args!("{mean_ms:.1}"),
                max_ms = format_args!("{max_ms:.1}"),
                budget_ms = format_args!("{budget_ms:.1}"),
                ticks = self.count,
                "render loop is behind its frame budget — input starves behind it"
            );
        } else {
            tracing::debug!(
                fps = format_args!("{fps:.1}"),
                target_fps = self.target_fps,
                mean_ms = format_args!("{mean_ms:.1}"),
                max_ms = format_args!("{max_ms:.1}"),
                "render pacing healthy"
            );
        }

        self.count = 0;
        self.sum = Duration::ZERO;
        self.max = Duration::ZERO;
    }
}
