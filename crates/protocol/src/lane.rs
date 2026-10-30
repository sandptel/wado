//! The fallback lane's budget (Decision Log `2026-10-27`): everything that rides the relay socket
//! — shells, Claude, clipboard, notifications, file listings, a trackpad with no direct path — is
//! held to **64 KB/s per seat, each way**, so the relay is never a free TURN.
//!
//! The relay enforces it (cap `lane`); the daemon paces its own side a little under it, so a
//! relay that enforces never has to queue, and one that predates the cap is still not flooded.
//! Both use [`Pace`]. Nothing is ever dropped: a frame over budget waits.
//!
//! No clock in here: the caller passes "now" in seconds on any monotonic clock, so this compiles
//! for the browser too.

/// What the relay allows each seat, each way, in bytes per second.
pub const LANE_BYTES_PER_SEC: u32 = 64 * 1024;
/// How far ahead of the rate a seat may run at the relay. Sized from what a session's start
/// really sends, measured 2026-10-30: the apps list alone is ~0.9 MB (icons inline; ~1.2 MB
/// sealed), and F6's speed test ~530 KB sealed — which must measure the path, not this cap (held
/// to 64 KB/s it would read 0.5 Mbps and refuse every join; `FLOOR_KBPS` is 1.5 Mbps).
///
/// **Why it must be this big:** the relay cannot reorder sealed frames. A frame the relay has to
/// hold makes every frame behind it wait too — the SDP answer of a reconnect included. So the
/// relay must never have to hold one from a daemon that paces itself: the daemon's model
/// ([`DAEMON_BURST_BYTES`], [`DAEMON_BYTES_PER_SEC`]) stays inside this one, and the gap between
/// them is the room its urgent frames take without being delayed. A burst is no TURN: past it,
/// the long-run rate is still 64 KB/s, and the relay keeps one budget per seat across reconnects.
pub const LANE_BURST_BYTES: u32 = 2 << 20;

/// What the daemon allows itself on the lane — inside the relay's numbers, so the small frames it
/// lets jump the queue (an SDP answer, a session verb) still fit in the relay's budget.
pub const DAEMON_BYTES_PER_SEC: u32 = 56 * 1024;
pub const DAEMON_BURST_BYTES: u32 = 1280 * 1024;

/// A generic cell rate pacer: one number, the time the budget is spent up to. A frame of `n`
/// bytes spends `n / rate` seconds; a pacer may run up to `burst / rate` seconds ahead of now.
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    rate: f64,
    ahead: f64,
    /// Seconds (on the caller's clock) the budget is spent up to.
    spent_to: f64,
}

impl Pace {
    pub fn new(bytes_per_sec: u32, burst_bytes: u32) -> Self {
        let rate = f64::from(bytes_per_sec.max(1));
        Self {
            rate,
            ahead: f64::from(burst_bytes) / rate,
            spent_to: f64::NEG_INFINITY,
        }
    }

    /// The relay's budget for one seat, one way.
    pub fn relay() -> Self {
        Self::new(LANE_BYTES_PER_SEC, LANE_BURST_BYTES)
    }

    /// The daemon's own budget for one room.
    pub fn daemon() -> Self {
        Self::new(DAEMON_BYTES_PER_SEC, DAEMON_BURST_BYTES)
    }

    /// How long a frame of `bytes` must wait from `now` before it may go. Zero: at once.
    /// Does not spend anything.
    pub fn wait(&self, now: f64, bytes: usize) -> f64 {
        let start = self.spent_to.max(now);
        let after = start + bytes as f64 / self.rate;
        (after - now - self.ahead).max(0.0)
    }

    /// Spend `bytes` as of `now` (when the frame actually goes).
    pub fn spend(&mut self, now: f64, bytes: usize) {
        self.spent_to = self.spent_to.max(now) + bytes as f64 / self.rate;
    }

    /// [`Pace::wait`], then [`Pace::spend`] as of when it may go: the wait, in seconds.
    pub fn take(&mut self, now: f64, bytes: usize) -> f64 {
        let w = self.wait(now, bytes);
        self.spend(now + w, bytes);
        w
    }

    /// Is the budget spent right now (a frame of any size would wait)? For "the lane is busy".
    pub fn saturated(&self, now: f64) -> bool {
        self.spent_to - now >= self.ahead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_goes_at_once_then_the_rate_holds() {
        let mut p = Pace::new(1000, 4000);
        // 4 KB of burst: four 1 KB frames at t=0 wait for nothing.
        for _ in 0..4 {
            assert_eq!(p.take(0.0, 1000), 0.0);
        }
        // The fifth waits one second, the sixth two.
        assert!((p.take(0.0, 1000) - 1.0).abs() < 1e-9);
        assert!((p.take(0.0, 1000) - 2.0).abs() < 1e-9);
        assert!(p.saturated(0.0));
    }

    #[test]
    fn idle_time_earns_the_burst_back_but_no_more() {
        let mut p = Pace::new(1000, 2000);
        p.take(0.0, 2000);
        assert!(p.wait(0.0, 1000) > 0.0);
        // A long silence refills only up to the burst.
        assert_eq!(p.take(100.0, 2000), 0.0);
        assert!((p.wait(100.0, 1000) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_frame_bigger_than_the_burst_still_goes_and_pays_for_itself() {
        let mut p = Pace::relay();
        // 3 MB at once: it goes after its overrun, and the next frame pays the debt.
        let w = p.take(0.0, 3 << 20);
        let want =
            ((3u64 << 20) - u64::from(LANE_BURST_BYTES)) as f64 / f64::from(LANE_BYTES_PER_SEC);
        assert!((w - want).abs() < 1e-6, "{w} vs {want}");
        assert!(p.wait(w, 1) > 0.0);
    }

    #[test]
    fn sustained_throughput_is_the_rate() {
        // 10 s of a sender that never stops: what got through is the rate plus one burst.
        let mut p = Pace::relay();
        let (mut t, mut sent) = (0.0, 0usize);
        while t < 10.0 {
            t += p.take(t, 16 * 1024);
            sent += 16 * 1024;
        }
        let rate = (sent - LANE_BURST_BYTES as usize) as f64 / t;
        assert!(
            (rate - f64::from(LANE_BYTES_PER_SEC)).abs() / f64::from(LANE_BYTES_PER_SEC) < 0.05,
            "{rate}"
        );
    }

    #[test]
    fn the_daemon_runs_under_the_relay() {
        assert!(DAEMON_BYTES_PER_SEC < LANE_BYTES_PER_SEC);
        assert!(DAEMON_BURST_BYTES < LANE_BURST_BYTES);
    }
}
