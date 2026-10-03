//! Redundant input, put back in order ("Redundant input", Display & stream).
//!
//! The viewer sends every discrete input event (keys, buttons, touch down/up) twice: on the
//! reliable input channel and on an unreliable, unordered fast one, each copy carrying the same
//! sequence number `q`. Whichever copy arrives first is used — so a reliable packet stuck behind a
//! retransmit no longer holds the touch behind it for a round trip, which on a jittery link is
//! touch that stalls and then jumps.
//!
//! The one thing a race must not do is reorder: a fast copy of "up" beating a delayed "down" would
//! be a broken click. So events are released strictly in `q` order — a later one waits for the
//! earlier, which the reliable channel guarantees will come.

use std::collections::BTreeMap;

#[derive(Default)]
pub struct InputOrder {
    next: Option<u64>,
    held: BTreeMap<u64, serde_json::Value>,
}

impl InputOrder {
    /// One arriving copy. Returns the events now releasable, in order (empty for a duplicate or a
    /// copy that must wait for an earlier one).
    pub fn arrive(&mut self, q: u64, ev: serde_json::Value) -> Vec<serde_json::Value> {
        let next = *self.next.get_or_insert(q);
        if q < next || self.held.contains_key(&q) {
            return Vec::new(); // the other copy got here first
        }
        self.held.insert(q, ev);
        let mut out = Vec::new();
        let mut n = next;
        while let Some(e) = self.held.remove(&n) {
            out.push(e);
            n += 1;
        }
        self.next = Some(n);
        // A gap that never fills (a viewer that stopped sending reliably) must not hold input
        // forever: past 64 waiting, release from the oldest held.
        if self.held.len() > 64 {
            if let Some((&k, _)) = self.held.iter().next() {
                self.next = Some(k);
                out.extend(self.arrive_flush());
            }
        }
        out
    }

    fn arrive_flush(&mut self) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        let Some(mut n) = self.next else { return out };
        while let Some(e) = self.held.remove(&n) {
            out.push(e);
            n += 1;
        }
        self.next = Some(n);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::InputOrder;
    use serde_json::json;

    #[test]
    fn first_copy_wins_duplicates_drop_order_holds() {
        let mut o = InputOrder::default();
        assert_eq!(o.arrive(1, json!("down")), vec![json!("down")]);
        assert!(
            o.arrive(1, json!("down")).is_empty(),
            "the second copy is a duplicate"
        );
        // The fast copy of 3 beats 2: held, not applied out of order.
        assert!(o.arrive(3, json!("up")).is_empty());
        assert_eq!(o.arrive(2, json!("move")), vec![json!("move"), json!("up")]);
        assert!(
            o.arrive(3, json!("up")).is_empty(),
            "late reliable copy of 3: duplicate"
        );
        assert!(o.arrive(2, json!("move")).is_empty());
        assert_eq!(o.arrive(4, json!("key")), vec![json!("key")]);
    }
}
