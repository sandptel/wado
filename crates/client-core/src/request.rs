//! The wire half of "every action shows its state": each command gets an op id, the message
//! carries it as the additive `ack` where it has one, and each op ends in exactly one outcome —
//! the answer, or `NoAnswer` once its deadline passes. No button waits forever.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Failed(String),
    /// The computer did not answer in time.
    NoAnswer,
}

#[derive(Default)]
pub struct Ops {
    next: u64,
    /// op → deadline, ms on the caller's clock.
    pending: BTreeMap<u64, u64>,
}

impl Ops {
    /// A fresh op id; never 0 (0 is "no ack" on the wire).
    pub fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// Wait for `op` until `at_ms`.
    pub fn deadline(&mut self, op: u64, at_ms: u64) {
        self.pending.insert(op, at_ms);
    }

    /// `op` was answered: its outcome, or `None` if it already ended (a late answer is dropped).
    pub fn answer(&mut self, op: u64, outcome: Outcome) -> Option<(u64, Outcome)> {
        self.pending.remove(&op).map(|_| (op, outcome))
    }

    /// Every op whose deadline has passed, each ended as `NoAnswer`.
    pub fn expired(&mut self, now_ms: u64) -> Vec<(u64, Outcome)> {
        let gone: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, &at)| at <= now_ms)
            .map(|(&op, _)| op)
            .collect();
        gone.into_iter()
            .map(|op| {
                self.pending.remove(&op);
                (op, Outcome::NoAnswer)
            })
            .collect()
    }

    /// The soonest deadline, to arm a timer for.
    pub fn soonest(&self) -> Option<u64> {
        self.pending.values().min().copied()
    }

    /// The wire `ack` for an op: its low 32 bits, never 0.
    pub fn ack(op: u64) -> u32 {
        (op as u32).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_answer_after_deadline() {
        let mut o = Ops::default();
        let a = o.next_id();
        let b = o.next_id();
        assert_ne!(a, 0);
        assert_ne!(a, b);
        o.deadline(a, 1000);
        o.deadline(b, 5000);
        assert_eq!(o.soonest(), Some(1000));
        assert!(o.expired(999).is_empty());
        assert_eq!(o.expired(1000), vec![(a, Outcome::NoAnswer)]);
        // A late answer for an op that already ended is dropped: one outcome per op.
        assert_eq!(o.answer(a, Outcome::Done), None);
        assert_eq!(o.answer(b, Outcome::Done), Some((b, Outcome::Done)));
        assert!(o.expired(10_000).is_empty());
    }
}
