//! The input arbiter: several devices driving one `wl_seat` at once (co-control, Decision Log
//! `2026-10-30`; spec `docs/superpowers/specs/2026-10-30-multi-client-design.md` §D4).
//!
//! Every event arrives tagged with its [`Origin`] — a number the *server* gives each peer
//! connection, so a device cannot speak as another. The arbiter decides, with no Wayland in
//! sight, what reaches the seat:
//!
//! - **Touch** ids are per device, so `(origin, id)` is remapped to a slot of its own: two phones'
//!   first finger are two contacts, not one.
//! - **Keyboard lease.** Whoever pressed the first key still held owns the keyboard until all of
//!   its keys are up; another device's presses meanwhile are dropped. One xkb state per keyboard
//!   means A's held Shift would otherwise shift B's typing.
//! - **Pointer lease.** While a device holds a button, drags a window or pinches, the others'
//!   pointer motion, buttons and scroll are dropped. Otherwise the last mover wins, and the
//!   pointer jumps to it — which is what co-control means.
//! - **Pointer lock** (relative motion, games) is the lead's alone.
//! - **A device leaving** releases everything it held: keys, buttons, contacts, a drag.
//!
//! Pure: [`Arbiter::admit`] and [`Arbiter::gone`] return the events to synthesize, which is what
//! lets every rule above be a unit test.

use std::collections::HashMap;

use wado_protocol::{InputEvent, PointerButton, TouchPhase};

/// Who sent an event, as the server numbers peer connections. Opaque to the compositor.
pub type Origin = u16;

/// Input with no device behind it worth telling apart: direct mode, and every caller from before
/// origins existed.
pub const PRIMARY: Origin = 0;

fn button_id(b: PointerButton) -> u8 {
    match b {
        PointerButton::Left => 0,
        PointerButton::Middle => 1,
        PointerButton::Right => 2,
    }
}

/// A drag or pinch in progress, and where it last was — so a device leaving mid-gesture can end
/// it where it stood.
#[derive(Debug, Clone, Copy)]
enum Gesture {
    Drag { x: f64, y: f64 },
    Pinch { x: f64, y: f64 },
}

#[derive(Debug, Default)]
pub struct Arbiter {
    lead: Option<Origin>,
    /// `(origin, its id)` → `(slot, last x, last y)`.
    touches: HashMap<(Origin, u32), (u32, f64, f64)>,
    next_slot: u32,
    /// Held keys and who pressed them.
    keys: HashMap<u32, Origin>,
    /// Held buttons: `(origin, button)` → where it was pressed.
    buttons: HashMap<(Origin, u8), (PointerButton, f64, f64)>,
    gesture: Option<(Origin, Gesture)>,
    last: Option<Origin>,
    /// Events dropped by a lease, for the log.
    pub dropped: u64,
}

impl Arbiter {
    /// The session's lead: only it may move a locked pointer.
    pub fn set_lead(&mut self, origin: Origin) {
        self.lead = Some(origin);
    }

    /// The device whose input arrived last — the one whose soft keyboard should rise.
    pub fn last_origin(&self) -> Option<Origin> {
        self.last
    }

    /// Someone other than `origin` holds the keyboard.
    fn keyboard_held_by_other(&self, origin: Origin) -> bool {
        self.keys.values().any(|o| *o != origin)
    }

    /// Someone other than `origin` holds the pointer: a button down, a drag, a pinch.
    fn pointer_held_by_other(&self, origin: Origin) -> bool {
        self.buttons.keys().any(|(o, _)| *o != origin)
            || self.gesture.is_some_and(|(o, _)| o != origin)
    }

    fn drop_it(&mut self, origin: Origin, why: &str) -> Vec<InputEvent> {
        self.dropped += 1;
        tracing::debug!(origin, why, "arbiter: input dropped");
        Vec::new()
    }

    /// What one event from `origin` becomes: itself, remapped, or nothing.
    pub fn admit(&mut self, origin: Origin, ev: InputEvent) -> Vec<InputEvent> {
        let out = match ev {
            InputEvent::Ping { .. } => return vec![ev],
            InputEvent::Key { code, pressed } => match (pressed, self.keys.get(&code).copied()) {
                (true, _) if self.keyboard_held_by_other(origin) => {
                    return self.drop_it(origin, "keyboard held by another device");
                }
                (true, _) => {
                    self.keys.insert(code, origin);
                    vec![ev]
                }
                (false, Some(o)) if o != origin => {
                    return self.drop_it(origin, "release of another device's key");
                }
                (false, _) => {
                    self.keys.remove(&code);
                    vec![ev]
                }
            },
            InputEvent::Touch { id, phase, x, y } => {
                let gesture_other = self.gesture.is_some_and(|(o, _)| o != origin);
                match phase {
                    TouchPhase::Down if gesture_other => {
                        return self.drop_it(origin, "another device is dragging");
                    }
                    TouchPhase::Down => {
                        let slot = match self.touches.get(&(origin, id)) {
                            Some((s, ..)) => *s,
                            None => self.alloc_slot(),
                        };
                        self.touches.insert((origin, id), (slot, x, y));
                        vec![InputEvent::Touch {
                            id: slot,
                            phase,
                            x,
                            y,
                        }]
                    }
                    TouchPhase::Motion => match self.touches.get_mut(&(origin, id)) {
                        Some(t) => {
                            t.1 = x;
                            t.2 = y;
                            vec![InputEvent::Touch {
                                id: t.0,
                                phase,
                                x,
                                y,
                            }]
                        }
                        None => return self.drop_it(origin, "motion of a contact never down"),
                    },
                    TouchPhase::Up => match self.touches.remove(&(origin, id)) {
                        Some((slot, ..)) => vec![InputEvent::Touch {
                            id: slot,
                            phase,
                            x,
                            y,
                        }],
                        None => return self.drop_it(origin, "lift of a contact never down"),
                    },
                }
            }
            InputEvent::CancelTouch { .. } => {
                // `wl_touch.cancel` is global: only send it when this device's contacts are the
                // only ones down. Otherwise its own contacts are lifted and the others' kept.
                let mine: Vec<(u32, (u32, f64, f64))> = self
                    .touches
                    .iter()
                    .filter(|((o, _), _)| *o == origin)
                    .map(|((_, id), v)| (*id, *v))
                    .collect();
                let others = self.touches.keys().any(|(o, _)| *o != origin);
                for (id, _) in &mine {
                    self.touches.remove(&(origin, *id));
                }
                if others {
                    mine.into_iter()
                        .map(|(_, (slot, x, y))| InputEvent::Touch {
                            id: slot,
                            phase: TouchPhase::Up,
                            x,
                            y,
                        })
                        .collect()
                } else {
                    vec![ev]
                }
            }
            InputEvent::PointerMotion { .. } | InputEvent::Scroll { .. } => {
                if self.pointer_held_by_other(origin) {
                    return self.drop_it(origin, "pointer held by another device");
                }
                vec![ev]
            }
            InputEvent::PointerRelative { .. } => {
                if self.lead.is_some_and(|l| l != origin) {
                    return self.drop_it(origin, "pointer lock is the lead's");
                }
                if self.pointer_held_by_other(origin) {
                    return self.drop_it(origin, "pointer held by another device");
                }
                vec![ev]
            }
            InputEvent::Button {
                x,
                y,
                button,
                pressed,
            } => {
                let id = button_id(button);
                if pressed {
                    if self.pointer_held_by_other(origin) {
                        return self.drop_it(origin, "pointer held by another device");
                    }
                    self.buttons.insert((origin, id), (button, x, y));
                    vec![ev]
                } else if self.buttons.remove(&(origin, id)).is_some() {
                    vec![ev]
                } else if self.buttons.keys().any(|(_, b)| *b == id) {
                    return self.drop_it(origin, "release of another device's button");
                } else {
                    vec![ev]
                }
            }
            InputEvent::WindowDrag { phase, x, y } => {
                match self.gesture_step(origin, phase, Gesture::Drag { x, y }) {
                    true => vec![ev],
                    false => return self.drop_it(origin, "another device is dragging"),
                }
            }
            InputEvent::Pinch { phase, x, y, .. } => {
                match self.gesture_step(origin, phase, Gesture::Pinch { x, y }) {
                    true => vec![ev],
                    false => return self.drop_it(origin, "another device is pinching"),
                }
            }
            InputEvent::StripView { .. }
            | InputEvent::GamepadButton { .. }
            | InputEvent::GamepadAxis { .. } => vec![ev],
        };
        self.last = Some(origin);
        out
    }

    /// A drag or pinch step: whether it passes, keeping the lease in step.
    fn gesture_step(&mut self, origin: Origin, phase: TouchPhase, g: Gesture) -> bool {
        match (phase, self.gesture) {
            (TouchPhase::Down, Some((o, _))) if o != origin => false,
            (_, None) if !matches!(phase, TouchPhase::Down) => {
                // A move or end of a gesture this arbiter never saw begin: let it through only
                // if nobody else holds the pointer.
                !self.pointer_held_by_other(origin)
            }
            (TouchPhase::Down, _) => {
                if self.buttons.keys().any(|(o, _)| *o != origin) {
                    return false;
                }
                self.gesture = Some((origin, g));
                true
            }
            (TouchPhase::Motion, Some((o, _))) if o == origin => {
                self.gesture = Some((origin, g));
                true
            }
            (TouchPhase::Up, Some((o, _))) if o == origin => {
                self.gesture = None;
                true
            }
            _ => false,
        }
    }

    fn alloc_slot(&mut self) -> u32 {
        loop {
            let s = self.next_slot;
            self.next_slot = self.next_slot.wrapping_add(1);
            if !self.touches.values().any(|(slot, ..)| *slot == s) {
                return s;
            }
        }
    }

    /// `origin` left: everything it held, released.
    pub fn gone(&mut self, origin: Origin) -> Vec<InputEvent> {
        let mut out = Vec::new();
        let keys: Vec<u32> = self
            .keys
            .iter()
            .filter(|(_, o)| **o == origin)
            .map(|(k, _)| *k)
            .collect();
        for code in keys {
            self.keys.remove(&code);
            out.push(InputEvent::Key {
                code,
                pressed: false,
            });
        }
        let buttons: Vec<(u8, (PointerButton, f64, f64))> = self
            .buttons
            .iter()
            .filter(|((o, _), _)| *o == origin)
            .map(|((_, b), v)| (*b, *v))
            .collect();
        for (b, (button, x, y)) in buttons {
            self.buttons.remove(&(origin, b));
            out.push(InputEvent::Button {
                x,
                y,
                button,
                pressed: false,
            });
        }
        let touches: Vec<(u32, (u32, f64, f64))> = self
            .touches
            .iter()
            .filter(|((o, _), _)| *o == origin)
            .map(|((_, id), v)| (*id, *v))
            .collect();
        for (id, (slot, x, y)) in touches {
            self.touches.remove(&(origin, id));
            out.push(InputEvent::Touch {
                id: slot,
                phase: TouchPhase::Up,
                x,
                y,
            });
        }
        if let Some((o, g)) = self.gesture {
            if o == origin {
                self.gesture = None;
                out.push(match g {
                    Gesture::Drag { x, y } => InputEvent::WindowDrag {
                        phase: TouchPhase::Up,
                        x,
                        y,
                    },
                    Gesture::Pinch { x, y } => InputEvent::Pinch {
                        phase: TouchPhase::Up,
                        x,
                        y,
                        scale: 1.0,
                        rotation: 0.0,
                    },
                });
            }
        }
        if self.last == Some(origin) {
            self.last = None;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Origin = 1;
    const B: Origin = 2;

    fn key(code: u32, pressed: bool) -> InputEvent {
        InputEvent::Key { code, pressed }
    }
    fn touch(id: u32, phase: TouchPhase) -> InputEvent {
        InputEvent::Touch {
            id,
            phase,
            x: 0.5,
            y: 0.5,
        }
    }
    fn button(pressed: bool) -> InputEvent {
        InputEvent::Button {
            x: 0.1,
            y: 0.2,
            button: PointerButton::Left,
            pressed,
        }
    }
    fn slot(evs: &[InputEvent]) -> u32 {
        match evs {
            [InputEvent::Touch { id, .. }] => *id,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn two_devices_first_finger_are_two_contacts() {
        let mut a = Arbiter::default();
        let sa = slot(&a.admit(A, touch(0, TouchPhase::Down)));
        let sb = slot(&a.admit(B, touch(0, TouchPhase::Down)));
        assert_ne!(sa, sb);
        assert_eq!(slot(&a.admit(B, touch(0, TouchPhase::Motion))), sb);
        assert_eq!(slot(&a.admit(A, touch(0, TouchPhase::Up))), sa);
        // A's contact is gone; its stray motion is not B's.
        assert!(a.admit(A, touch(0, TouchPhase::Motion)).is_empty());
    }

    #[test]
    fn a_key_during_anothers_lease_is_dropped() {
        let mut a = Arbiter::default();
        assert_eq!(a.admit(A, key(42, true)).len(), 1, "A holds Shift");
        assert!(
            a.admit(B, key(30, true)).is_empty(),
            "B's 'a' is not shifted"
        );
        assert!(
            a.admit(B, key(42, false)).is_empty(),
            "nor can B lift A's Shift"
        );
        assert_eq!(a.admit(A, key(42, false)).len(), 1);
        assert_eq!(a.admit(B, key(30, true)).len(), 1, "lease over: B types");
        assert_eq!(a.dropped, 2);
    }

    #[test]
    fn a_drag_lease_blocks_the_others_pointer() {
        let mut a = Arbiter::default();
        assert_eq!(a.admit(A, button(true)).len(), 1);
        let mv = InputEvent::PointerMotion { x: 0.9, y: 0.9 };
        assert!(a.admit(B, mv.clone()).is_empty(), "A is dragging");
        assert!(a.admit(B, button(true)).is_empty());
        assert_eq!(a.admit(A, mv.clone()).len(), 1, "A's own drag moves");
        assert_eq!(a.admit(A, button(false)).len(), 1);
        assert_eq!(a.admit(B, mv).len(), 1, "free again: last mover wins");
        // A window drag is a lease too.
        let drag = |phase| InputEvent::WindowDrag {
            phase,
            x: 0.3,
            y: 0.3,
        };
        assert_eq!(a.admit(B, drag(TouchPhase::Down)).len(), 1);
        assert!(a.admit(A, drag(TouchPhase::Down)).is_empty());
        assert!(a.admit(A, touch(5, TouchPhase::Down)).is_empty());
        assert_eq!(a.admit(B, drag(TouchPhase::Up)).len(), 1);
    }

    #[test]
    fn leaving_releases_everything_held() {
        let mut a = Arbiter::default();
        a.admit(A, key(29, true));
        a.admit(A, button(true));
        let s = slot(&a.admit(A, touch(3, TouchPhase::Down)));
        a.admit(B, touch(0, TouchPhase::Down));
        let out = a.gone(A);
        assert!(out.contains(&key(29, false)));
        assert!(
            out.iter()
                .any(|e| matches!(e, InputEvent::Button { pressed: false, .. }))
        );
        assert!(out.iter().any(|e| matches!(
            e,
            InputEvent::Touch { id, phase: TouchPhase::Up, .. } if *id == s
        )));
        assert_eq!(out.len(), 3, "B's contact is untouched");
        // And B can type straight away.
        assert_eq!(a.admit(B, key(30, true)).len(), 1);
    }

    #[test]
    fn pointer_lock_is_the_leads() {
        let mut a = Arbiter::default();
        let rel = InputEvent::PointerRelative { dx: 3.0, dy: 1.0 };
        assert_eq!(a.admit(B, rel.clone()).len(), 1, "no lead yet: as before");
        a.set_lead(A);
        assert!(a.admit(B, rel.clone()).is_empty());
        assert_eq!(a.admit(A, rel).len(), 1);
    }

    #[test]
    fn a_cancel_lifts_only_its_own_contacts_when_others_are_down() {
        let mut a = Arbiter::default();
        let sa = slot(&a.admit(A, touch(0, TouchPhase::Down)));
        a.admit(B, touch(0, TouchPhase::Down));
        let out = a.admit(A, InputEvent::CancelTouch { id: 0 });
        assert!(
            matches!(out.as_slice(), [InputEvent::Touch { id, phase: TouchPhase::Up, .. }] if *id == sa)
        );
        // Alone, a cancel is the seat's global cancel.
        let out = a.admit(B, InputEvent::CancelTouch { id: 0 });
        assert!(matches!(out.as_slice(), [InputEvent::CancelTouch { .. }]));
    }

    #[test]
    fn one_device_is_never_held_back() {
        // The single-viewer case must be exactly as before.
        let mut a = Arbiter::default();
        for ev in [
            key(42, true),
            key(30, true),
            key(30, false),
            key(42, false),
            button(true),
            InputEvent::PointerMotion { x: 0.2, y: 0.2 },
            button(false),
            InputEvent::PointerRelative { dx: 1.0, dy: 1.0 },
        ] {
            assert_eq!(a.admit(PRIMARY, ev.clone()), vec![ev]);
        }
        assert_eq!(a.last_origin(), Some(PRIMARY));
    }
}
