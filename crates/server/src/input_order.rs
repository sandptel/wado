//! Redundant input, put back together ("Redundant input", Display & stream).
//!
//! The viewer sends every input event it numbers (`q`) on the reliable input channel *and* on an
//! unordered, unreliable fast one; the first copy to arrive is used. What "used" means depends on
//! the kind of event, because holding everything in strict order made the fast lane glitchy:
//! one lost fast copy held every later event until its slow reliable copy came, then released
//! them all at once — scroll and drag stalled and jumped (reported 2026-10-03).
//!
//! - **Scroll steps** (`scroll` without `stop`) add up, so order does not matter: released the
//!   moment the first copy arrives, never waiting.
//! - **Moves** (`touch` motion, `window_drag` motion) are absolute positions: released at once if
//!   newer than the last one applied for the same finger, dropped if older. Only the newest matters.
//! - **Everything else** (down, up, keys, buttons, the scroll's `stop`) is strict: released only
//!   once every lower `q` has been seen, so a lift can never beat its press.

use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub struct InputOrder {
    /// Every `q` below this has been seen.
    floor: Option<u64>,
    seen: HashSet<u64>,
    strict: BTreeMap<u64, serde_json::Value>,
    newest_move: HashMap<String, u64>,
    pub stats: Stats,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub released: u64,
    pub duplicates: u64,
    pub stale_moves: u64,
    pub held: u64,
}

enum Kind {
    Commutative,
    Move(String),
    Strict,
}

fn kind(ev: &serde_json::Value) -> Kind {
    let t = ev.get("t").and_then(|v| v.as_str()).unwrap_or("");
    let phase = ev.get("phase").and_then(|v| v.as_str()).unwrap_or("");
    match (t, phase) {
        ("scroll", _) if !ev.get("stop").and_then(|v| v.as_bool()).unwrap_or(false) => {
            Kind::Commutative
        }
        ("touch", "motion") => Kind::Move(format!(
            "touch:{}",
            ev.get("id").map_or(String::new(), |v| v.to_string())
        )),
        ("window_drag", "motion") => Kind::Move("window_drag".into()),
        _ => Kind::Strict,
    }
}

impl InputOrder {
    /// One arriving copy. Returns the events to apply now, in the order to apply them.
    pub fn arrive(&mut self, q: u64, ev: serde_json::Value) -> Vec<serde_json::Value> {
        let floor = *self.floor.get_or_insert(q);
        if q < floor || !self.seen.insert(q) {
            self.stats.duplicates += 1;
            return Vec::new();
        }
        let mut out = Vec::new();
        match kind(&ev) {
            Kind::Commutative => out.push(ev),
            Kind::Move(key) => {
                let last = self.newest_move.entry(key).or_insert(0);
                if q > *last {
                    *last = q;
                    out.push(ev);
                } else {
                    self.stats.stale_moves += 1;
                }
            }
            Kind::Strict => {
                self.strict.insert(q, ev);
            }
        }
        // Advance the floor over everything seen; strict events become releasable as it passes.
        let mut f = floor;
        while self.seen.contains(&f) {
            self.seen.remove(&f);
            f += 1;
        }
        self.floor = Some(f);
        let ready: Vec<u64> = self.strict.range(..f).map(|(k, _)| *k).collect();
        for k in ready {
            if let Some(e) = self.strict.remove(&k) {
                out.push(e);
            }
        }
        if !self.strict.is_empty() {
            self.stats.held += 1;
        }
        // A gap that never fills must not hold input for good: past 64 waiting, give up on it.
        if self.strict.len() > 64 {
            if let Some(&k) = self.strict.keys().next() {
                self.floor = Some(k);
                self.seen.retain(|&s| s > k);
                let rest: Vec<u64> = self.strict.keys().copied().collect();
                for k in rest {
                    if let Some(e) = self.strict.remove(&k) {
                        out.push(e);
                    }
                }
            }
        }
        self.stats.released += out.len() as u64;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::InputOrder;
    use serde_json::json;

    fn down() -> serde_json::Value {
        json!({"t":"touch","id":1,"phase":"down"})
    }
    fn mv(x: f64) -> serde_json::Value {
        json!({"t":"touch","id":1,"phase":"motion","x":x})
    }
    fn up() -> serde_json::Value {
        json!({"t":"touch","id":1,"phase":"up"})
    }
    fn sc(dy: f64) -> serde_json::Value {
        json!({"t":"scroll","dy":dy})
    }

    #[test]
    fn strict_events_never_reorder_and_copies_drop() {
        let mut o = InputOrder::default();
        assert_eq!(o.arrive(1, down()), vec![down()]);
        assert!(o.arrive(1, down()).is_empty(), "second copy is a duplicate");
        // The fast copy of the lift (3) beats a lost move (2): the lift waits.
        assert!(o.arrive(3, up()).is_empty());
        assert_eq!(o.arrive(2, mv(5.0)), vec![mv(5.0), up()]);
    }

    #[test]
    fn scroll_steps_never_wait_for_a_gap() {
        let mut o = InputOrder::default();
        assert_eq!(o.arrive(1, sc(1.0)), vec![sc(1.0)]);
        // 2 is lost on the fast lane: 3 and 4 still apply at once.
        assert_eq!(o.arrive(3, sc(3.0)), vec![sc(3.0)]);
        assert_eq!(o.arrive(4, sc(4.0)), vec![sc(4.0)]);
        assert_eq!(
            o.arrive(2, sc(2.0)),
            vec![sc(2.0)],
            "the late one still counts — deltas add up"
        );
        assert!(o.arrive(3, sc(3.0)).is_empty(), "but only once");
    }

    #[test]
    fn moves_apply_newest_first_and_drop_stale() {
        let mut o = InputOrder::default();
        assert_eq!(o.arrive(1, down()), vec![down()]);
        assert_eq!(
            o.arrive(3, mv(3.0)),
            vec![mv(3.0)],
            "no waiting for the lost move 2"
        );
        assert!(
            o.arrive(2, mv(2.0)).is_empty(),
            "older position: stale, dropped"
        );
        assert_eq!(o.stats.stale_moves, 1);
    }
}
