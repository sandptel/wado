//! State that is pushed, not polled: a capability's latest state on a `watch` channel, refreshed
//! at most as often as its readers need.
//!
//! Readers (the Computer page, home widgets, a desktop app) subscribe, or take the latest value,
//! and never cause I/O. Whoever wants it fresher calls [`Published::fresh`] with how old it may
//! be: concurrent asks share one fetch, and an ask within the age is answered by the last one.
//! That is what turns "every poll of every viewer runs every tool" into "each tool runs at most
//! once per age, however many ask".

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, watch};

/// The latest state of one capability.
pub struct Published<S> {
    tx: watch::Sender<S>,
    /// When the last fetch started. Held across a fetch, so a second ask waits for the first
    /// instead of starting its own.
    last: Mutex<Option<Instant>>,
    /// Fetches run, for measurement and tests.
    fetches: AtomicU64,
}

impl<S: Clone + PartialEq + Send + Sync> Published<S> {
    pub fn new(init: S) -> Self {
        Published {
            tx: watch::channel(init).0,
            last: Mutex::new(None),
            fetches: AtomicU64::new(0),
        }
    }

    /// A receiver that sees every change from now on.
    pub fn subscribe(&self) -> watch::Receiver<S> {
        self.tx.subscribe()
    }

    /// The latest state. No I/O.
    pub fn get(&self) -> S {
        self.tx.borrow().clone()
    }

    /// Replace the state; subscribers are woken only when it changed.
    pub fn publish(&self, next: S) {
        self.tx.send_if_modified(|cur| {
            if *cur == next {
                return false;
            }
            *cur = next;
            true
        });
    }

    /// Change the state in place; subscribers are woken only when `f` says it changed.
    pub fn modify(&self, f: impl FnOnce(&mut S) -> bool) {
        self.tx.send_if_modified(f);
    }

    /// Make sure the state is no older than `within`, running `fetch` only when it is.
    ///
    /// "No older" means a fetch that *started* at most `within` before this call — so
    /// `Duration::ZERO` asks for a fetch begun after this call, which is what reading back a
    /// change just made needs. Concurrent callers queue behind one fetch and are then answered by
    /// it when it is recent enough.
    pub async fn fresh<F, Fut>(&self, within: Duration, fetch: F)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = S>,
    {
        let asked = Instant::now();
        let mut last = self.last.lock().await;
        if let Some(t) = *last
            && (t > asked || (!within.is_zero() && asked.duration_since(t) <= within))
        {
            return;
        }
        *last = Some(Instant::now());
        self.fetches.fetch_add(1, Ordering::Relaxed);
        let next = fetch().await;
        self.publish(next);
    }

    /// How many fetches have run.
    pub fn fetches(&self) -> u64 {
        self.fetches.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn concurrent_asks_share_one_fetch() {
        let p = Arc::new(Published::new(0u32));
        let slow = || async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            7
        };
        let (a, b) = (Arc::clone(&p), Arc::clone(&p));
        tokio::join!(
            a.fresh(Duration::from_secs(1), slow),
            b.fresh(Duration::from_secs(1), slow)
        );
        assert_eq!(p.fetches(), 1);
        assert_eq!(p.get(), 7);
    }

    #[tokio::test]
    async fn an_ask_within_the_age_is_answered_by_the_last_fetch() {
        let p = Published::new(0u32);
        p.fresh(Duration::from_secs(2), || async { 1 }).await;
        p.fresh(Duration::from_secs(2), || async { 2 }).await;
        assert_eq!((p.fetches(), p.get()), (1, 1));
    }

    #[tokio::test]
    async fn zero_always_fetches_again() {
        let p = Published::new(0u32);
        p.fresh(Duration::from_secs(60), || async { 1 }).await;
        p.fresh(Duration::ZERO, || async { 2 }).await;
        assert_eq!((p.fetches(), p.get()), (2, 2));
    }

    #[tokio::test]
    async fn subscribers_wake_only_on_change() {
        let p = Published::new(1u32);
        let mut rx = p.subscribe();
        p.publish(1);
        assert!(!rx.has_changed().unwrap());
        p.publish(2);
        assert!(rx.has_changed().unwrap());
        assert_eq!(*rx.borrow_and_update(), 2);
    }
}
