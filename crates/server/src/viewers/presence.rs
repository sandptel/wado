//! Ghost cursors (spec §D4): where each participant last pointed, for the others to see as a
//! labelled dot. The compositor still draws no cursor — the dots are the client's.
//!
//! Read off the input each participant sends (the server tags it, so nobody can move another's
//! dot), sent at most [`HZ`] times a second per participant on its own unordered, unreliable data
//! channel (`wado-presence`) — never the input channels, never the relay (invariant 1). A
//! participant never receives its own.

use std::collections::HashMap;

use wado_protocol::{InputEvent, TouchPhase, multi::Presence};

use crate::session::membership::Origin;

/// Per participant, at most this often.
pub const HZ: u64 = 20;
const GAP_MS: u64 = 1000 / HZ;

/// Where an input event points, and with what — or nothing (keys, the gamepad, a lift).
pub fn spot(ev: &InputEvent) -> Option<(f64, f64, &'static str)> {
    match ev {
        InputEvent::PointerMotion { x, y }
        | InputEvent::Button { x, y, .. }
        | InputEvent::Scroll { x, y, .. } => Some((*x, *y, "pointer")),
        InputEvent::Touch { x, y, phase, .. } if *phase != TouchPhase::Up => {
            Some((*x, *y, "touch"))
        }
        _ => None,
    }
}

#[derive(Debug, Default)]
struct Dot {
    p: Presence,
    dirty: bool,
    sent_ms: u64,
    gone: bool,
}

#[derive(Debug, Default)]
pub struct Board {
    dots: HashMap<Origin, Dot>,
}

impl Board {
    /// `origin` pointed at `(x, y)`.
    pub fn point(&mut self, origin: Origin, name: &str, hue: u8, x: f64, y: f64, kind: &str) {
        let d = self.dots.entry(origin).or_default();
        d.p = Presence {
            id: origin,
            name: name.to_string(),
            hue,
            x: x.clamp(0.0, 1.0),
            y: y.clamp(0.0, 1.0),
            kind: kind.to_string(),
        };
        d.dirty = true;
        d.gone = false;
    }

    /// `origin` left: its dot is taken down on the next send.
    pub fn gone(&mut self, origin: Origin) {
        if let Some(d) = self.dots.get_mut(&origin) {
            d.p.kind.clear();
            d.dirty = true;
            d.gone = true;
        }
    }

    /// The dots due now — changed, and not sent within the last `1/HZ` s. A dot taken down is
    /// sent once more (empty `kind`) and forgotten.
    pub fn due(&mut self, now: u64) -> Vec<Presence> {
        let mut out = Vec::new();
        for d in self.dots.values_mut() {
            if d.dirty && (d.gone || now.saturating_sub(d.sent_ms) >= GAP_MS) {
                d.dirty = false;
                d.sent_ms = now;
                out.push(d.p.clone());
            }
        }
        self.dots.retain(|_, d| !(d.gone && !d.dirty));
        out
    }
}

/// What `viewer` is sent of a batch: everyone's but its own.
pub fn for_viewer(batch: &[Presence], viewer: Origin) -> Vec<Presence> {
    batch.iter().filter(|p| p.id != viewer).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_most_twenty_a_second_per_participant() {
        let mut b = Board::default();
        b.point(1, "A", 0, 0.1, 0.1, "pointer");
        assert_eq!(b.due(1000).len(), 1);
        b.point(1, "A", 0, 0.2, 0.2, "pointer");
        assert!(b.due(1020).is_empty(), "inside 50 ms: held");
        b.point(1, "A", 0, 0.3, 0.3, "pointer");
        let d = b.due(1050);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].x, 0.3, "the latest position, not a backlog");
        assert!(b.due(1200).is_empty(), "nothing changed: nothing sent");
    }

    #[test]
    fn nobody_sees_their_own_dot() {
        let mut b = Board::default();
        b.point(1, "A", 0, 0.1, 0.1, "pointer");
        b.point(2, "B", 1, 0.9, 0.9, "touch");
        let batch = b.due(5000);
        let to_a = for_viewer(&batch, 1);
        assert_eq!(to_a.len(), 1);
        assert_eq!(to_a[0].name, "B");
    }

    #[test]
    fn a_dot_leaves_with_its_participant() {
        let mut b = Board::default();
        b.point(1, "A", 0, 0.1, 0.1, "pointer");
        b.due(1000);
        b.gone(1);
        let d = b.due(1001);
        assert_eq!(d.len(), 1, "sent at once, not after the gap");
        assert!(d[0].kind.is_empty());
        assert!(b.due(9000).is_empty(), "and forgotten");
    }

    #[test]
    fn only_pointing_input_moves_a_dot() {
        assert!(
            spot(&InputEvent::Key {
                code: 30,
                pressed: true
            })
            .is_none()
        );
        assert_eq!(
            spot(&InputEvent::PointerMotion { x: 0.5, y: 0.25 }),
            Some((0.5, 0.25, "pointer"))
        );
        let up = InputEvent::Touch {
            id: 0,
            phase: TouchPhase::Up,
            x: 0.5,
            y: 0.5,
        };
        assert!(spot(&up).is_none());
    }
}
