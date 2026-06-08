//! Encoder-stall detection for the render loop (one job: count consecutive empty
//! encode ticks and signal when the encoder has gone silent).
//!
//! A VAAPI encoder under a very long GOP (on-demand keyframe mode) can enter a state
//! where every call to `submit` returns `Ok(None)` — no packets, no error — leaving
//! the viewer's stream frozen indefinitely. The [`StallWatchdog`] detects this by
//! counting consecutive empty ticks; the caller should call [`StallWatchdog::reset`]
//! on a successful packet and trigger `downgrade_pipeline` when [`StallWatchdog::tick`]
//! returns `true`.

/// Tracks consecutive empty `Ok(None)` ticks from the encoder.
///
/// `threshold` is set to ~fps/2 (~0.5 s of silence), with a floor of 10 ticks so
/// very low frame-rate sessions don't trigger spuriously. Steady-state encoding
/// always yields at least one packet per tick because every rendered frame is
/// submitted unconditionally (there is no damage-skip gate in `render_tick`), so
/// the threshold will not fire under normal operation.
pub struct StallWatchdog {
    consecutive_empty: u32,
    threshold: u32,
}

impl StallWatchdog {
    /// Create a new watchdog calibrated to `fps`. The threshold is `max(fps/2, 10)`.
    pub fn new(fps: u32) -> Self {
        let threshold = (fps / 2).max(10);
        Self { consecutive_empty: 0, threshold }
    }

    /// Record one tick result. Returns `true` when the stall threshold is exceeded,
    /// indicating the caller should treat the encoder as stalled and downgrade.
    ///
    /// - `got_packet = true`  → resets the counter; never returns `true`.
    /// - `got_packet = false` → increments the counter; returns `true` at threshold.
    pub fn tick(&mut self, got_packet: bool) -> bool {
        if got_packet {
            self.consecutive_empty = 0;
            false
        } else {
            self.consecutive_empty += 1;
            self.consecutive_empty >= self.threshold
        }
    }

    /// Reset the counter (call after a successful downgrade so the new encoder starts fresh).
    pub fn reset(&mut self) {
        self.consecutive_empty = 0;
    }

    /// How many consecutive empty ticks have been counted (for log messages).
    pub fn consecutive_empty(&self) -> u32 {
        self.consecutive_empty
    }
}
