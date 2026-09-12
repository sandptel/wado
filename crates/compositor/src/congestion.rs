// SPDX-License-Identifier: AGPL-3.0-only
//! Shedding render ticks when the network cannot take the frames we are already making.
//!
//! **What this is not.** It is not congestion control and it does not adapt the bitrate. The
//! encoder's rate is fixed when the session opens (`ffmpeg/mod.rs` sets `bit_rate`/`rc_max_rate`/
//! `rc_min_rate` and a VBV derived from them; x264 takes it through the builder), and neither
//! backend exposes a runtime change — so the only lever the render loop actually holds is *how
//! often it produces a frame at all*. Real bandwidth estimation is a separate, larger piece of
//! work; see `plan/research.md` R6.
//!
//! **Why it is still worth having.** Measured 2026-09-12 on a tethered mobile link: the pump
//! filled and `ChannelSink` discarded 420 encoded frames in one session — six seconds of video
//! that was captured, composited, encoded, and then thrown away. Every one of those cost a GPU
//! readback and an encode for nothing, and the viewer saw stale frames regardless. Not producing
//! them is strictly less work for exactly the same output, which is what makes this safe: the
//! worst case of shedding is the case we are already in.
//!
//! **The signal** is `ChannelSink`'s drop counter, which lives in this process — no round trip,
//! no RTCP, nothing to negotiate. It is monotonic, so what matters is the *delta* over a window;
//! a level would latch on the first hiccup of the session and never clear.
//!
//! **The law is asymmetric on purpose**: halve the rate on the first window that drops anything,
//! and ease back one step only after several consecutive clean windows. Symmetric control on a
//! link this bursty oscillates, and oscillation between smooth and stuttering reads worse to a
//! viewer than a steady lower rate.

/// Ticks per decision window. At 60 fps this is one second; at 120, half of one. Deliberately
/// counted in ticks rather than wall time: the thing being controlled is the tick, and a window
/// that shortens as the rate rises is the responsiveness you want.
pub const WINDOW_TICKS: u32 = 60;

/// Never shed more than three ticks in four. Below that the stream stops reading as video, and a
/// viewer would rather have a low frame rate than a slideshow that looks like a freeze.
pub const MAX_DIVISOR: u32 = 4;

/// Consecutive clean windows before easing back one step.
const RECOVER_WINDOWS: u32 = 3;

/// How often the render loop renders: 1 = every tick, 2 = every other, 4 = every fourth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divisor(pub u32);

impl Divisor {
    pub const ONE: Divisor = Divisor(1);
}

/// The controller's state between windows.
#[derive(Debug, Clone, Copy)]
pub struct Congestion {
    divisor: u32,
    /// Ticks since the last decision.
    tick: u32,
    /// `ChannelSink`'s drop count at the last decision, so we can take a delta.
    last_dropped: u64,
    /// Consecutive windows with no drops.
    clean: u32,
}

impl Default for Congestion {
    fn default() -> Self {
        Self { divisor: 1, tick: 0, last_dropped: 0, clean: 0 }
    }
}

impl Congestion {
    /// Reset for a new session. The drop counter belongs to the old sink and means nothing here.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn divisor(&self) -> u32 {
        self.divisor
    }

    /// Should this tick render? Advances the window and re-decides when it closes.
    ///
    /// `dropped_total` is the sink's monotonic counter.
    pub fn should_render(&mut self, dropped_total: u64) -> bool {
        let render = self.tick % self.divisor == 0;
        self.tick += 1;
        if self.tick >= WINDOW_TICKS {
            let delta = dropped_total.saturating_sub(self.last_dropped);
            self.last_dropped = dropped_total;
            self.tick = 0;
            let (divisor, clean) = decide(delta, self.divisor, self.clean);
            if divisor != self.divisor {
                tracing::info!(
                    from = self.divisor,
                    to = divisor,
                    dropped_in_window = delta,
                    "shedding render ticks — the pump could not take the frames we were making"
                );
            }
            self.divisor = divisor;
            self.clean = clean;
        }
        render
    }
}

/// The decision itself, kept pure so it is testable without a session, a link or a GPU.
///
/// Returns the new divisor and the new clean-window count.
fn decide(dropped_in_window: u64, divisor: u32, clean: u32) -> (u32, u32) {
    if dropped_in_window > 0 {
        // Back off immediately and forget any accumulated recovery: one dropped frame means the
        // pump is at its limit right now, and the credit earned before that is stale.
        ((divisor * 2).min(MAX_DIVISOR), 0)
    } else if divisor > 1 && clean + 1 >= RECOVER_WINDOWS {
        // One step back toward full rate, and the recovery counter restarts — so climbing from
        // 4 to 1 takes RECOVER_WINDOWS clean windows per step, not one for the whole way.
        (divisor / 2, 0)
    } else {
        (divisor, clean + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_link_never_sheds() {
        let mut c = Congestion::default();
        for i in 0..(WINDOW_TICKS * 10) {
            assert!(c.should_render(0), "tick {i} was shed on a link with no drops");
        }
        assert_eq!(c.divisor(), 1);
    }

    #[test]
    fn one_dropped_frame_halves_the_rate() {
        // The asymmetry that matters: a single drop is enough, because by the time drops are
        // plural the viewer has already seen it.
        assert_eq!(decide(1, 1, 0), (2, 0));
        assert_eq!(decide(60, 2, 0), (4, 0));
    }

    #[test]
    fn shedding_stops_at_the_floor() {
        // Past MAX_DIVISOR the stream stops being video. Sustained drops must not walk it to 8.
        assert_eq!(decide(60, 4, 0), (MAX_DIVISOR, 0));
        assert_eq!(decide(60, MAX_DIVISOR, 0), (MAX_DIVISOR, 0));
    }

    #[test]
    fn recovery_is_slower_than_back_off() {
        // Two clean windows are not enough; the third releases one step.
        assert_eq!(decide(0, 4, 0), (4, 1));
        assert_eq!(decide(0, 4, 1), (4, 2));
        assert_eq!(decide(0, 4, 2), (2, 0));
        // And the counter restarts, so 4 -> 1 costs 2 * RECOVER_WINDOWS clean windows.
        assert_eq!(decide(0, 2, 0), (2, 1));
        assert_eq!(decide(0, 2, 2), (1, 0));
    }

    #[test]
    fn a_drop_during_recovery_resets_the_credit() {
        // Otherwise a link that drops one frame per window would still climb back to full rate.
        assert_eq!(decide(1, 4, 2), (MAX_DIVISOR, 0));
    }

    #[test]
    fn at_divisor_two_exactly_half_the_ticks_render() {
        let mut c = Congestion::default();
        // Drive it into shedding: one window with drops.
        for _ in 0..WINDOW_TICKS {
            c.should_render(0);
        }
        for _ in 0..WINDOW_TICKS {
            c.should_render(5);
        }
        assert_eq!(c.divisor(), 2);
        let rendered = (0..WINDOW_TICKS).filter(|_| c.should_render(5)).count();
        assert_eq!(rendered as u32, WINDOW_TICKS / 2);
    }

    #[test]
    fn a_monotonic_counter_is_read_as_a_delta() {
        // The bug this guards: `dropped` only ever rises, so a controller comparing it to zero
        // would latch on the first hiccup and shed for the rest of the session.
        let mut c = Congestion::default();
        for _ in 0..WINDOW_TICKS {
            c.should_render(0);
        }
        // One window with drops: back off.
        for _ in 0..WINDOW_TICKS {
            c.should_render(100);
        }
        assert_eq!(c.divisor(), 2);
        // The counter stays at 100 — no *new* drops — so this must read as clean and recover.
        for _ in 0..(WINDOW_TICKS * RECOVER_WINDOWS) {
            c.should_render(100);
        }
        assert_eq!(c.divisor(), 1, "a static counter was misread as ongoing congestion");
    }
}
