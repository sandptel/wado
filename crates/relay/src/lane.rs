//! The fallback lane's cap, enforced (cap `lane`; Decision Log `2026-10-27`): every frame the
//! relay forwards for a seat — or for a seatless control link — is paced to
//! [`LANE_BYTES_PER_SEC`] each way, after a [`LANE_BURST_BYTES`] burst, so the relay can carry
//! shells, clipboard and listings for a device with no direct path without ever becoming a free
//! TURN server.
//!
//! **A frame over budget waits; it is never dropped.** The envelope numbers every sealed frame,
//! so a dropped one would end the link. Waiting is backpressure: on the client's side the relay
//! simply reads its socket later, on the daemon's the seat's inbox fills. The daemon paces itself
//! under this cap (`wado_protocol::lane::DAEMON_BYTES_PER_SEC`), so a current daemon never meets
//! it; it is what holds an older daemon, or a client, to the budget.

use std::time::{Duration, Instant};

use tracing::info;
use wado_protocol::lane::Pace;
pub use wado_protocol::lane::{LANE_BURST_BYTES, LANE_BYTES_PER_SEC};

/// One way of one seat.
pub struct Lane {
    pace: Pace,
    epoch: Instant,
    /// Who and which way, for the log line.
    label: String,
    /// When the cap was last said to bite, so it is said once a minute at most.
    told: Option<Instant>,
}

impl Lane {
    pub fn new(label: String) -> Self {
        Self {
            pace: Pace::relay(),
            epoch: Instant::now(),
            label,
            told: None,
        }
    }

    /// How long a frame of `bytes` must wait now, and the budget spent for it.
    pub fn take(&mut self, bytes: usize) -> Duration {
        let now = self.epoch.elapsed().as_secs_f64();
        let wait = self.pace.take(now, bytes);
        if wait > 0.0
            && self
                .told
                .is_none_or(|t| t.elapsed() > Duration::from_secs(60))
        {
            self.told = Some(Instant::now());
            info!(
                "lane: {} is at its {} KB/s cap — a {bytes}-byte frame waits {:.0} ms",
                self.label,
                LANE_BYTES_PER_SEC / 1024,
                wait * 1000.0
            );
        }
        Duration::from_secs_f64(wait)
    }

    /// Wait out a frame of `bytes`.
    pub async fn admit(&mut self, bytes: usize) {
        let wait = self.take(bytes);
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_traffic_never_waits_and_a_flood_is_held_to_the_rate() {
        let mut l = Lane::new("test".into());
        // A session's worth of signalling: nothing waits.
        for _ in 0..50 {
            assert!(l.take(2_000).is_zero());
        }
        // A megabyte at once: everything past the burst waits at the rate.
        let mut waited = Duration::ZERO;
        for _ in 0..64 {
            waited = l.take(16 * 1024);
        }
        let want = (100_000 + 64 * 16 * 1024 - LANE_BURST_BYTES as usize) as f64
            / f64::from(LANE_BYTES_PER_SEC);
        assert!(
            (waited.as_secs_f64() - want).abs() < 0.05,
            "{waited:?} vs {want}"
        );
    }
}
