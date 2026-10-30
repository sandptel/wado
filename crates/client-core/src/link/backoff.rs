//! Reconnect delays, pure: quick at first because most drops are a blip, capped low enough that a
//! phone coming back from a dead zone is linked again in seconds. Never gives up.
//!
//! 250 ms, 500 ms, 1 s, 2 s, 4 s, then 8 s; each ±20 % so a relay restart is not met by every
//! device at the same instant.

use std::time::Duration;

const FIRST_MS: f64 = 250.0;
const CAP_MS: f64 = 8000.0;
const JITTER: f64 = 0.2;

/// The delay before attempt `attempt` (1 is the first retry). `jitter` in -1..=1 (clamped).
pub fn delay(attempt: u32, jitter: f64) -> Duration {
    let n = attempt.saturating_sub(1).min(16);
    let base = (FIRST_MS * f64::from(1u32 << n)).min(CAP_MS);
    let j = jitter.clamp(-1.0, 1.0) * JITTER;
    Duration::from_millis((base * (1.0 + j)).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(a: u32, j: f64) -> u64 {
        delay(a, j).as_millis() as u64
    }

    #[test]
    fn backoff_is_capped_and_jittered() {
        let plain: Vec<u64> = (1..=8).map(|a| ms(a, 0.0)).collect();
        assert_eq!(plain, [250, 500, 1000, 2000, 4000, 8000, 8000, 8000]);
        assert_eq!(ms(0, 0.0), 250, "attempt 0 is the first");
        assert_eq!(ms(1000, 0.0), 8000, "never past the cap");
        assert_eq!(ms(3, 1.0), 1200);
        assert_eq!(ms(3, -1.0), 800);
        assert_eq!(ms(6, 5.0), 9600, "jitter is clamped to ±20 %");
    }
}
