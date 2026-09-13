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
//! **Two signals, two speeds.**
//!
//! The first is `ChannelSink`'s drop counter, which lives in this process — no round trip, no
//! RTCP, nothing to negotiate. It says the *network* could not take what we made. It is
//! monotonic, so what matters is the delta over a window; a level would latch on the first
//! hiccup of the session and never clear.
//!
//! The second is the viewer saying its *decoder* is saturated. Nothing in this process can see
//! that: measured 2026-09-12, the server pushed 90 fps into a phone managing 15 for 86 seconds
//! with every server-side metric perfect throughout. The client computes it (`js/health.js`) and
//! sends a settled boolean; see `CompositorCommand::ViewerStrain`.
//!
//! **The law is asymmetric on purpose, and differently so for each signal**: drops halve the
//! rate on the first window that sees any, because by the time drops are plural the viewer has
//! already seen it. Strain steps down only after several consecutive windows asserting it,
//! because it is a level rather than an event — it arrives at 1 Hz, stays latched between
//! updates, and a window at 90 fps and divisor 4 closes in under a second, so treating it as a
//! trigger would walk straight to the floor no matter what the phone was doing. Recovery is
//! slow in both directions, and strain blocks it outright.

/// Ticks per decision window. At 60 fps this is one second; at 120, half of one. Deliberately
/// counted in ticks rather than wall time: the thing being controlled is the tick, and a window
/// that shortens as the rate rises is the responsiveness you want.
pub const WINDOW_TICKS: u32 = 60;

/// Never shed more than three ticks in four. Below that the stream stops reading as video, and a
/// viewer would rather have a low frame rate than a slideshow that looks like a freeze.
pub const MAX_DIVISOR: u32 = 4;

/// Consecutive clean windows before easing back one step.
const RECOVER_WINDOWS: u32 = 3;

/// Consecutive strained windows before stepping down one. Equal to [`RECOVER_WINDOWS`]
/// deliberately: the viewer's decoder is a slow-moving thing compared with a burst of pump
/// drops, so it gets a symmetric, unhurried response rather than the drop path's immediate halve.
const STRAIN_WINDOWS: u32 = 3;

/// How much more patient recovery gets after each strain-driven step down, and the ceiling on it.
///
/// **Why recovery cannot simply be symmetric here.** The viewer measures its decoder *under the
/// mitigation*, so shedding destroys the evidence that justified shedding — and the release is
/// then "the symptom went away", which it always does. Measured 2026-09-12, both cycles:
///
/// | | strain asserted at | after shedding to 1-in-4 | released |
/// |---|---|---|---|
/// | 16:17 | 4.8 of 5.7 Mbps arriving | 816 kbps | 3 windows later |
/// | 16:18 | 6.0 of 5.7 Mbps arriving | 1.6 Mbps | 3 windows later |
///
/// Every release happened at divisor 4 and none at 1, which is the signature of a loop feeding on
/// its own output. Slowing recovery by a constant only lengthens the period. Making the patience
/// *grow* converges: each probe back toward full rate costs one brief saturation, and the probes
/// get rarer until the rate sits where the phone can hold it.
const PATIENCE_FACTOR: u32 = 4;
/// ~3.5 minutes of clean windows at 90 fps. Far enough apart that a probe is not felt as pumping,
/// close enough that a phone which has genuinely cooled down still gets its frame rate back.
const PATIENCE_MAX: u32 = 320;

/// What moved the divisor. Carried into the log line so a `SHED` event stays attributable to a
/// side — the run that built the network/device/server attribution would be undone by a shed
/// that could have come from either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The pump could not take the frames — the link.
    Drops,
    /// The viewer says its decoder is saturated — the phone.
    Strain,
    /// Easing back toward full rate.
    Recover,
}

impl Reason {
    fn as_str(self) -> &'static str {
        match self {
            Reason::Drops => "the pump could not take the frames we were making",
            Reason::Strain => "the viewer says its decoder is saturated",
            Reason::Recover => "clean windows — easing back toward full rate",
        }
    }
}

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
    /// Consecutive windows in which the viewer reported strain.
    strain: u32,
    /// Clean windows currently required to climb one step. Grows with each strain-driven step
    /// down; see [`PATIENCE_FACTOR`].
    patience: u32,
}

impl Default for Congestion {
    fn default() -> Self {
        Self { divisor: 1, tick: 0, last_dropped: 0, clean: 0, strain: 0, patience: RECOVER_WINDOWS }
    }
}

impl Congestion {
    /// Reset for a new session. The drop counter belongs to the old sink and means nothing here.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The **same** viewer's media path came back. Keep what was learned about its decoder;
    /// forget only the grudge.
    ///
    /// **The bug this closes, measured live 2026-09-13.** `ViewerAttached(true)` called
    /// [`Congestion::reset`], so every reconnect restored the full frame rate. On a mobile link
    /// that reconnects every one to four minutes — which is exactly the link this branch was
    /// built for — the phone was re-flooded at 90 fps on every return, saturated again, and had
    /// to walk the divisor back down from scratch. The daemon log shows it plainly: every
    /// `viewer attached` is followed by a fresh `shedding … from=1`.
    ///
    /// It also explains the decode spikes recorded as I18 — 109 ms eight seconds after one
    /// reconnect, 55 ms thirty seconds after another. Not a mysterious decoder collapse: the
    /// server had just gone back to sending four times as many frames.
    ///
    /// The reasoning for the reset was "a new decoder starts with no history and must not inherit
    /// a shed", and that is right for a *new viewer*. A viewer that reconnects forty seconds later
    /// is not a new viewer, and its decoder is the same silicon that could not keep up before.
    ///
    /// So the divisor survives and the **patience** does not. Patience grows with every strain
    /// (see `PATIENCE_FACTOR`) to stop the loop feeding on its own output, but holding a long
    /// session's accumulated patience across a reconnect would make recovery glacial for a viewer
    /// that has genuinely improved — or for a different, faster device. Keeping the rate and
    /// giving recovery a fresh start is the combination that is wrong in neither direction.
    pub fn reattach(&mut self) {
        let divisor = self.divisor;
        *self = Self { divisor, ..Self::default() };
    }

    pub fn divisor(&self) -> u32 {
        self.divisor
    }

    /// Should this tick render? Advances the window and re-decides when it closes.
    ///
    /// `dropped_total` is the sink's monotonic counter. `strained` is the viewer's latest word
    /// on its own decoder — a latched level, not an event.
    pub fn should_render(&mut self, dropped_total: u64, strained: bool) -> bool {
        let render = self.tick % self.divisor == 0;
        self.tick += 1;
        if self.tick >= WINDOW_TICKS {
            let delta = dropped_total.saturating_sub(self.last_dropped);
            self.last_dropped = dropped_total;
            self.tick = 0;
            let next = decide(delta, strained, self.divisor, self.clean, self.strain, self.patience);
            if next.divisor != self.divisor {
                tracing::info!(
                    from = self.divisor,
                    to = next.divisor,
                    dropped_in_window = delta,
                    strained,
                    patience = next.patience,
                    "shedding render ticks — {}",
                    next.reason.as_str()
                );
            }
            self.divisor = next.divisor;
            self.clean = next.clean;
            self.strain = next.strain;
            self.patience = next.patience;
        }
        render
    }
}

/// The outcome of one decision window.
struct Decision {
    divisor: u32,
    clean: u32,
    strain: u32,
    patience: u32,
    reason: Reason,
}

/// The decision itself, kept pure so it is testable without a session, a link or a GPU.
fn decide(
    dropped_in_window: u64,
    strained: bool,
    divisor: u32,
    clean: u32,
    strain: u32,
    patience: u32,
) -> Decision {
    if dropped_in_window > 0 {
        // Back off immediately and forget any accumulated recovery: one dropped frame means the
        // pump is at its limit right now, and the credit earned before that is stale. Checked
        // first because it is the faster signal and the one measured locally.
        return Decision {
            divisor: (divisor * 2).min(MAX_DIVISOR),
            clean: 0,
            strain: if strained { strain + 1 } else { 0 },
            patience,
            reason: Reason::Drops,
        };
    }
    if strained {
        // A level, so it is counted, not acted on. Stepping down on the first strained window
        // would reach the floor in well under a second at a high frame rate, long before the
        // viewer could have measured the effect of the previous step.
        let strain = strain + 1;
        if divisor < MAX_DIVISOR && strain >= STRAIN_WINDOWS {
            return Decision {
                divisor: divisor * 2,
                clean: 0,
                strain: 0,
                // The probe that follows must be rarer than the last one, or the loop above
                // repeats forever at the same period.
                patience: (patience * PATIENCE_FACTOR).min(PATIENCE_MAX),
                reason: Reason::Strain,
            };
        }
        // Held, and recovery credit is not accrued: a strained viewer must never climb back.
        return Decision { divisor, clean: 0, strain, patience, reason: Reason::Strain };
    }
    if divisor > 1 && clean + 1 >= patience {
        // One step back toward full rate, and the counter restarts — so climbing from 4 to 1
        // costs the full patience per step, not one wait for the whole way.
        return Decision {
            divisor: divisor / 2,
            clean: 0,
            strain: 0,
            patience,
            reason: Reason::Recover,
        };
    }
    Decision { divisor, clean: clean + 1, strain: 0, patience, reason: Reason::Recover }
}

#[cfg(test)]
mod tests {
    // ── reattach: the same viewer coming back ────────────────────────────────

    #[test]
    fn a_reconnect_does_not_restore_the_full_frame_rate() {
        // The live regression of 2026-09-13: every `viewer attached` was followed by a fresh
        // `shedding ... from=1`, so a link that reconnects every two minutes re-floods the phone
        // every two minutes and it never stops walking the divisor back down.
        let mut c = Congestion::default();
        for _ in 0..(WINDOW_TICKS * STRAIN_WINDOWS * 2) {
            c.should_render(0, true);
        }
        let shed = c.divisor();
        assert!(shed > 1, "the setup must actually have shed something, got {shed}");
        c.reattach();
        assert_eq!(c.divisor(), shed, "a reconnect must keep the rate the decoder earned");
    }

    #[test]
    fn a_reconnect_forgets_the_grudge_but_not_the_rate() {
        // Patience grows with every strain so the loop cannot feed on its own output. Carrying a
        // long session's accumulated patience across a reconnect would make recovery glacial for
        // a viewer that has genuinely improved — or for a different, faster device.
        let mut c = Congestion::default();
        for _ in 0..(WINDOW_TICKS * STRAIN_WINDOWS * 4) {
            c.should_render(0, true);
        }
        let shed = c.divisor();
        c.reattach();
        assert_eq!(c.divisor(), shed);
        // With patience back at its floor, a clean run recovers rather than crawling.
        for _ in 0..(WINDOW_TICKS * (RECOVER_WINDOWS + 1)) {
            c.should_render(0, false);
        }
        assert!(c.divisor() < shed, "recovery should be possible again after a reattach");
    }

    #[test]
    fn a_brand_new_session_still_starts_at_full_rate() {
        // `reset` keeps its old meaning; only `reattach` is the softer one.
        let mut c = Congestion::default();
        for _ in 0..(WINDOW_TICKS * STRAIN_WINDOWS * 2) {
            c.should_render(0, true);
        }
        assert!(c.divisor() > 1);
        c.reset();
        assert_eq!(c.divisor(), 1);
    }

    use super::*;

    /// `decide` with no strain — the shape every pre-existing test was written against.
    fn drops(dropped: u64, divisor: u32, clean: u32) -> (u32, u32) {
        let d = decide(dropped, false, divisor, clean, 0, RECOVER_WINDOWS);
        (d.divisor, d.clean)
    }

    /// `decide` for the strain path, at whatever patience the caller is testing.
    fn strainy(divisor: u32, clean: u32, strain: u32, patience: u32) -> Decision {
        decide(0, true, divisor, clean, strain, patience)
    }

    #[test]
    fn a_clean_link_never_sheds() {
        let mut c = Congestion::default();
        for i in 0..(WINDOW_TICKS * 10) {
            assert!(c.should_render(0, false), "tick {i} was shed on a link with no drops");
        }
        assert_eq!(c.divisor(), 1);
    }

    #[test]
    fn one_dropped_frame_halves_the_rate() {
        // The asymmetry that matters: a single drop is enough, because by the time drops are
        // plural the viewer has already seen it.
        assert_eq!(drops(1, 1, 0), (2, 0));
        assert_eq!(drops(60, 2, 0), (4, 0));
    }

    #[test]
    fn shedding_stops_at_the_floor() {
        // Past MAX_DIVISOR the stream stops being video. Sustained drops must not walk it to 8.
        assert_eq!(drops(60, 4, 0), (MAX_DIVISOR, 0));
        assert_eq!(drops(60, MAX_DIVISOR, 0), (MAX_DIVISOR, 0));
    }

    #[test]
    fn recovery_is_slower_than_back_off() {
        // Two clean windows are not enough; the third releases one step.
        assert_eq!(drops(0, 4, 0), (4, 1));
        assert_eq!(drops(0, 4, 1), (4, 2));
        assert_eq!(drops(0, 4, 2), (2, 0));
        // And the counter restarts, so 4 -> 1 costs 2 * RECOVER_WINDOWS clean windows.
        assert_eq!(drops(0, 2, 0), (2, 1));
        assert_eq!(drops(0, 2, 2), (1, 0));
    }

    #[test]
    fn a_drop_during_recovery_resets_the_credit() {
        // Otherwise a link that drops one frame per window would still climb back to full rate.
        assert_eq!(drops(1, 4, 2), (MAX_DIVISOR, 0));
    }

    #[test]
    fn at_divisor_two_exactly_half_the_ticks_render() {
        let mut c = Congestion::default();
        // Drive it into shedding: one window with drops.
        for _ in 0..WINDOW_TICKS {
            c.should_render(0, false);
        }
        for _ in 0..WINDOW_TICKS {
            c.should_render(5, false);
        }
        assert_eq!(c.divisor(), 2);
        let rendered = (0..WINDOW_TICKS).filter(|_| c.should_render(5, false)).count();
        assert_eq!(rendered as u32, WINDOW_TICKS / 2);
    }

    #[test]
    fn a_monotonic_counter_is_read_as_a_delta() {
        // The bug this guards: `dropped` only ever rises, so a controller comparing it to zero
        // would latch on the first hiccup and shed for the rest of the session.
        let mut c = Congestion::default();
        for _ in 0..WINDOW_TICKS {
            c.should_render(0, false);
        }
        // One window with drops: back off.
        for _ in 0..WINDOW_TICKS {
            c.should_render(100, false);
        }
        assert_eq!(c.divisor(), 2);
        // The counter stays at 100 — no *new* drops — so this must read as clean and recover.
        for _ in 0..(WINDOW_TICKS * RECOVER_WINDOWS) {
            c.should_render(100, false);
        }
        assert_eq!(c.divisor(), 1, "a static counter was misread as ongoing congestion");
    }

    // ── The viewer's decoder ──────────────────────────────────────────────────────────────

    #[test]
    fn strain_takes_several_windows_to_move_anything() {
        // The failure this guards is the one a latched 1 Hz boolean invites: at 90 fps with the
        // divisor already at 4, a window closes in under a second, so acting on the first
        // strained window would reach the floor before the phone could measure the last step.
        let mut d = strainy(1, 0, 0, RECOVER_WINDOWS);
        assert_eq!((d.divisor, d.strain), (1, 1));
        d = strainy(1, 0, d.strain, RECOVER_WINDOWS);
        assert_eq!((d.divisor, d.strain), (1, 2));
        d = strainy(1, 0, d.strain, RECOVER_WINDOWS);
        assert_eq!((d.divisor, d.strain), (2, 0), "the third strained window steps down");
    }

    #[test]
    fn strain_blocks_recovery_without_stepping_down_again_immediately() {
        // Held at 2 with the recovery credit refused — a strained viewer must never climb back.
        let d = strainy(2, 2, 0, RECOVER_WINDOWS);
        assert_eq!((d.divisor, d.clean), (2, 0));
    }

    #[test]
    fn strain_stops_at_the_floor_like_drops_do() {
        let d = strainy(MAX_DIVISOR, 0, STRAIN_WINDOWS, RECOVER_WINDOWS);
        assert_eq!(d.divisor, MAX_DIVISOR);
    }

    #[test]
    fn a_viewer_that_stops_complaining_recovers() {
        // Strain clears, the clean windows accrue, and the rate climbs back. Without this the
        // feature is a one-way ratchet and a single bad minute costs the rest of the session.
        let mut c = Congestion::default();
        for _ in 0..(WINDOW_TICKS * STRAIN_WINDOWS) {
            c.should_render(0, true);
        }
        assert_eq!(c.divisor(), 2, "three strained windows should have stepped down once");
        // Patience has grown to RECOVER_WINDOWS * PATIENCE_FACTOR by now, so the old
        // three-window wait is no longer enough — that is the point.
        for _ in 0..(WINDOW_TICKS * RECOVER_WINDOWS * PATIENCE_FACTOR) {
            c.should_render(0, false);
        }
        assert_eq!(c.divisor(), 1);
    }

    #[test]
    fn each_strain_step_makes_the_next_probe_rarer() {
        // The loop this breaks, measured 2026-09-12: shed, throughput collapses, the client stops
        // reporting strain *because* of the shed, recover, saturate, repeat — every release at
        // divisor 4, never at 1. A constant recovery delay only sets the period of that; growing
        // patience is what makes the probes rare enough to converge.
        let a = strainy(1, 0, STRAIN_WINDOWS - 1, RECOVER_WINDOWS);
        assert_eq!(a.divisor, 2);
        assert_eq!(a.patience, RECOVER_WINDOWS * PATIENCE_FACTOR);
        let b = strainy(2, 0, STRAIN_WINDOWS - 1, a.patience);
        assert_eq!(b.divisor, 4);
        assert_eq!(b.patience, RECOVER_WINDOWS * PATIENCE_FACTOR * PATIENCE_FACTOR);
        // Bounded, or a long session would eventually never recover at all.
        let mut p = b.patience;
        for _ in 0..10 {
            p = (p * PATIENCE_FACTOR).min(PATIENCE_MAX);
        }
        assert_eq!(p, PATIENCE_MAX);
    }

    #[test]
    fn a_pump_drop_does_not_inflate_strain_patience() {
        // Only the self-measuring signal needs the growing wait. The drop counter is observed
        // locally and is not affected by the mitigation, so its recovery stays quick.
        let d = decide(9, false, 1, 0, 0, RECOVER_WINDOWS);
        assert_eq!((d.divisor, d.patience), (2, RECOVER_WINDOWS));
    }

    #[test]
    fn drops_outrank_strain_and_are_attributed_as_such() {
        // Both signals at once. The link is the faster-moving fault and the one measured here,
        // so it wins the step and the log line — otherwise a network problem would be reported
        // to the user as their phone being too slow, which is the attribution this run exists
        // to keep straight.
        let d = decide(5, true, 1, 0, 0, RECOVER_WINDOWS);
        assert_eq!(d.divisor, 2);
        assert_eq!(d.reason, Reason::Drops);
        // And strain is still counted, so it is not starved of credit by a noisy link.
        assert_eq!(d.strain, 1);
    }
}
