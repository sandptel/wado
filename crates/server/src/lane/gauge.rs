//! How much bulk is waiting on a link's lane — and the wait a streaming producer takes before
//! adding more.
//!
//! The pump never stops reading its channel (that is what lets an SDP answer overtake a
//! transfer), so backpressure for a stream cannot come from the channel. It comes from here: a
//! producer that can flood — shell output, a file on the lane — waits for [`Gauge::room`] before
//! each piece, and a backlog stays a few seconds deep instead of minutes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Notify;

/// A producer waits while more than this is queued: about three seconds of the lane.
pub const ROOM_BYTES: usize = 160 * 1024;

#[derive(Clone, Default)]
pub struct Gauge(Arc<Inner>);

#[derive(Default)]
struct Inner {
    queued: AtomicUsize,
    drained: Notify,
}

impl Gauge {
    pub fn add(&self, n: usize) {
        self.0.queued.fetch_add(n, Ordering::SeqCst);
    }

    pub fn sub(&self, n: usize) {
        self.0.queued.fetch_sub(n, Ordering::SeqCst);
        self.0.drained.notify_waiters();
    }

    pub fn queued(&self) -> usize {
        self.0.queued.load(Ordering::SeqCst)
    }

    /// Is the lane backed up right now? (For a hint, or for a producer that would rather skip.)
    pub fn busy(&self) -> bool {
        self.queued() >= ROOM_BYTES
    }

    /// Wait until there is room for more.
    pub async fn room(&self) {
        loop {
            let drained = self.0.drained.notified();
            if !self.busy() {
                return;
            }
            drained.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_producer_waits_for_the_backlog_to_drain() {
        let g = Gauge::default();
        g.add(ROOM_BYTES);
        assert!(g.busy());
        let waiter = tokio::spawn({
            let g = g.clone();
            async move { g.room().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        g.sub(1);
        waiter.await.unwrap();
    }
}
