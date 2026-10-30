//! Is the relay loop still reading? A watchdog that does not run on the loop.
//!
//! The loop's own silence check (`timeout(RELAY_SILENCE, next())`) only runs when the loop gets
//! back to `next()`. A handler that never returns — a send into a full channel, the hang of
//! 2026-10-30 — keeps it from ever firing: the daemon said "connected" while the relay had long
//! since dropped it. So the loop stamps [`Reads`] each time it reads a frame, and a task of its
//! own ([`watch`]) wakes the loop's [`Stuck`] when nothing has been read for too long. The loop
//! drops whatever it was waiting on and reconnects.
//!
//! Armed only on a relay that pings (cap `ping`, every 15 s): on an older one a quiet link is
//! normal.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::warn;

/// How often the watchdog looks.
const CHECK: Duration = Duration::from_secs(5);

/// When the loop last read a frame from the relay.
#[derive(Clone)]
pub struct Reads(Arc<Inner>);

struct Inner {
    base: Instant,
    /// Milliseconds after `base`.
    last: AtomicU64,
    armed: AtomicBool,
}

impl Default for Reads {
    fn default() -> Self {
        Self(Arc::new(Inner {
            base: Instant::now(),
            last: AtomicU64::new(0),
            armed: AtomicBool::new(false),
        }))
    }
}

impl Reads {
    /// A frame was read.
    pub fn touch(&self) {
        let ms = self.0.base.elapsed().as_millis() as u64;
        self.0.last.store(ms, Ordering::Relaxed);
    }

    /// The relay pings: from now on a quiet loop is a stuck one.
    pub fn arm(&self) {
        self.touch();
        self.0.armed.store(true, Ordering::Relaxed);
    }

    /// How long since the last read, if armed.
    fn quiet(&self) -> Option<Duration> {
        if !self.0.armed.load(Ordering::Relaxed) {
            return None;
        }
        let now = self.0.base.elapsed().as_millis() as u64;
        Some(Duration::from_millis(
            now.saturating_sub(self.0.last.load(Ordering::Relaxed)),
        ))
    }
}

/// What the loop waits on beside its work: woken once the watchdog gives up on it.
pub type Stuck = Arc<Notify>;

/// The watchdog for one connection; aborted when dropped.
pub struct Watch(JoinHandle<()>);

impl Drop for Watch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Watch `reads`: once nothing has been read for longer than `limit`, say so and wake `stuck`.
pub fn watch(reads: Reads, limit: Duration, stuck: Stuck) -> Watch {
    Watch(tokio::spawn(async move {
        loop {
            tokio::time::sleep(CHECK).await;
            if let Some(quiet) = reads.quiet()
                && quiet > limit
            {
                warn!(
                    "relay client: loop stuck for {}s — reconnecting",
                    quiet.as_secs()
                );
                stuck.notify_one();
                return;
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: Duration = Duration::from_secs(50);

    #[tokio::test(start_paused = true)]
    async fn a_loop_that_stops_reading_is_woken() {
        let reads = Reads::default();
        let stuck = Stuck::default();
        reads.arm();
        let _w = watch(reads.clone(), LIMIT, Arc::clone(&stuck));
        // Reading every 15 s (the relay's pings): never stuck.
        for _ in 0..8 {
            tokio::time::sleep(Duration::from_secs(15)).await;
            reads.touch();
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(1), stuck.notified())
                .await
                .is_err()
        );
        // Then a handler that never returns: woken within one check of the limit.
        let t0 = Instant::now();
        stuck.notified().await;
        let took = t0.elapsed();
        assert!(
            took > LIMIT && took <= LIMIT + CHECK + Duration::from_secs(1),
            "{took:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_relay_that_never_pings_is_not_watched() {
        let stuck = Stuck::default();
        let _w = watch(Reads::default(), LIMIT, Arc::clone(&stuck));
        assert!(
            tokio::time::timeout(LIMIT * 4, stuck.notified())
                .await
                .is_err()
        );
    }
}
