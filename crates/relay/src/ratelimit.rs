//! Join rate limit — a token bucket per client IP.
//!
//! The Remote ID is ~30 bits and the relay is public, so unthrottled joins make guessing one an
//! afternoon's work. A bucket of 20 that refills one token every 3 s lets a real device
//! reconnect as fast as it ever needs to (a parked join holds one socket instead of knocking),
//! and holds a guesser to ~20 IDs a minute per address.
//!
//! The IP comes from [`crate::signaling::peer_ip`], which believes proxy headers only with
//! `--trust-proxy`. Behind a cloudflared tunnel *without* it, every join comes from
//! 127.0.0.1 and shares one bucket — so the tunnel deployment must set the flag.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

#[derive(Clone)]
pub struct JoinLimiter {
    buckets: Arc<DashMap<String, (f64, Instant)>>,
    burst: f64,
    refill: Duration,
}

impl JoinLimiter {
    pub fn new(burst: u32, refill: Duration) -> Self {
        Self {
            buckets: Arc::default(),
            burst: burst.max(1) as f64,
            refill: refill.max(Duration::from_millis(1)),
        }
    }

    /// Spend one token for `ip`. `Err(wait)` when there is none: how long until there is.
    pub fn take(&self, ip: &str) -> Result<(), Duration> {
        let now = Instant::now();
        // ponytail: an unbounded map swept when it gets big; an LRU if the relay ever sees
        // enough distinct addresses for the sweep to show up.
        if self.buckets.len() > 10_000 {
            let idle = self.refill.mul_f64(self.burst);
            self.buckets
                .retain(|_, (_, t)| now.duration_since(*t) < idle);
        }
        let mut b = self
            .buckets
            .entry(ip.to_string())
            .or_insert((self.burst, now));
        let (tokens, at) = *b;
        let tokens = (tokens + now.duration_since(at).as_secs_f64() / self.refill.as_secs_f64())
            .min(self.burst);
        if tokens >= 1.0 {
            *b = (tokens - 1.0, now);
            Ok(())
        } else {
            *b = (tokens, now);
            Err(self.refill.mul_f64(1.0 - tokens))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JoinLimiter;
    use std::time::Duration;

    #[test]
    fn a_burst_then_a_wait_per_address() {
        let l = JoinLimiter::new(3, Duration::from_secs(60));
        for _ in 0..3 {
            assert!(l.take("1.1.1.1").is_ok());
        }
        let wait = l.take("1.1.1.1").unwrap_err();
        assert!(wait > Duration::from_secs(50));
        // Another address has its own bucket.
        assert!(l.take("2.2.2.2").is_ok());
    }
}
