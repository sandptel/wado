//! The core's event queue: bounded by coalescing, never by dropping. Link, host and sessions
//! events for one computer replace their queued predecessor (newest wins, keeping its place);
//! pairing steps and answers are never merged, so none is lost. If the UI stalls, events wait.

use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::sync::Notify;

use crate::event::CoreEvent;

/// Past this, only coalescing keeps it in bounds (a stalled UI with steady link chatter).
pub const BOUND: usize = 256;

#[derive(Default)]
pub struct EventQueue {
    q: Mutex<VecDeque<CoreEvent>>,
    ready: Notify,
}

impl EventQueue {
    pub fn push(&self, ev: CoreEvent) {
        {
            let mut q = self.q.lock().unwrap();
            let slot = ev.slot().map(|(k, s)| (k, s.to_string()));
            let same = slot.and_then(|(k, s)| {
                q.iter()
                    .position(|e| e.slot().is_some_and(|(k2, s2)| k2 == k && s2 == s))
            });
            match same {
                Some(i) => q[i] = ev,
                None => q.push_back(ev),
            }
        }
        self.ready.notify_one();
    }

    pub fn try_pop(&self) -> Option<CoreEvent> {
        self.q.lock().unwrap().pop_front()
    }

    pub fn len(&self) -> usize {
        self.q.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The next event, waiting for one.
    pub async fn pop(&self) -> CoreEvent {
        loop {
            let ready = self.ready.notified();
            if let Some(e) = self.try_pop() {
                return e;
            }
            ready.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::LinkView;
    use crate::link::state::LinkState;
    use crate::pair::flow::PairStep;
    use crate::request::Outcome;

    fn link(key: &str, s: LinkState) -> CoreEvent {
        CoreEvent::Link {
            key: key.into(),
            view: LinkView::new(s, 0, vec![], String::new()),
        }
    }

    #[test]
    fn events_coalesce_per_key_newest_wins() {
        let q = EventQueue::default();
        q.push(link("a", LinkState::Dialing));
        q.push(link("b", LinkState::Dialing));
        for i in 0..1000 {
            q.push(link("a", LinkState::Open { since_ms: i }));
        }
        assert_eq!(q.len(), 2, "one per key");
        assert_eq!(
            q.try_pop(),
            Some(link("a", LinkState::Open { since_ms: 999 }))
        );
        assert_eq!(q.try_pop(), Some(link("b", LinkState::Dialing)));
        assert!(q.is_empty());
    }

    #[test]
    fn pair_and_answer_never_coalesce() {
        let q = EventQueue::default();
        q.push(CoreEvent::Pair {
            op: 1,
            step: PairStep::Dialing,
        });
        q.push(CoreEvent::Pair {
            op: 1,
            step: PairStep::Securing,
        });
        q.push(CoreEvent::Answer {
            op: 2,
            outcome: Outcome::Done,
        });
        q.push(CoreEvent::Answer {
            op: 2,
            outcome: Outcome::Done,
        });
        assert_eq!(q.len(), 4);
    }

    #[tokio::test]
    async fn pop_waits_for_a_push() {
        let q = std::sync::Arc::new(EventQueue::default());
        let q2 = q.clone();
        let t = tokio::spawn(async move { q2.pop().await });
        tokio::task::yield_now().await;
        q.push(link("a", LinkState::Dialing));
        assert_eq!(t.await.unwrap(), link("a", LinkState::Dialing));
    }
}
