//! The virtual multi-touch touchpad: device setup, and the logic that turns one frame of
//! [`Contact`]s into evdev slot events.

use std::{collections::HashSet, io};

use wado_protocol::pad::Contact;

use super::uinput::{self, EV_ABS, EV_KEY};

const INPUT_PROP_POINTER: u16 = 0x00;
const INPUT_PROP_BUTTONPAD: u16 = 0x02;

const BTN_LEFT: u16 = 0x110;
const BTN_TOUCH: u16 = 0x14a;
const BTN_TOOL_FINGER: u16 = 0x145;
const BTN_TOOL_QUINTTAP: u16 = 0x148;
const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
const BTN_TOOL_TRIPLETAP: u16 = 0x14e;
const BTN_TOOL_QUADTAP: u16 = 0x14f;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;

/// At most this many fingers tracked at once — one `ABS_MT_SLOT` per finger, 0..=4.
const SLOTS: usize = 5;

/// The `BTN_TOOL_*` code for `count` simultaneous fingers, or `None` for zero.
fn tool_code(count: usize) -> Option<u16> {
    match count {
        1 => Some(BTN_TOOL_FINGER),
        2 => Some(BTN_TOOL_DOUBLETAP),
        3 => Some(BTN_TOOL_TRIPLETAP),
        4 => Some(BTN_TOOL_QUADTAP),
        5 => Some(BTN_TOOL_QUINTTAP),
        _ => None,
    }
}

/// `v` (0..=65535) mapped onto the device's `0..=max` axis.
fn scale(v: u16, max: i32) -> i32 {
    (v as i64 * max as i64 / 65535) as i32
}

/// Persistent slot state, carried by the caller across frames (`Touchpad` owns one). A slot
/// holds the client-assigned id currently occupying it and the kernel tracking id handed out
/// for it; `next_tracking_id` is a counter, never the client id, because client ids are
/// reused across lift/re-press while a tracking id must not be (`ABS_MT_TRACKING_ID` is how
/// libinput tells a re-press apart from the same finger never having moved). `ignored` holds
/// client ids that showed up with no free slot — they stay locked out for their whole
/// lifetime, even once a slot frees, until they are absent from a frame at all.
#[derive(Default)]
pub struct Slots {
    slots: [Option<(u8, u16)>; SLOTS],
    next_tracking_id: u16,
    ignored: HashSet<u8>,
}

/// The evdev events that take the device from `state`'s current slot assignment to `next`'s,
/// mutating `state` in place. `w`/`h` are the device's axis maxima (already the `*10` device
/// range, not CSS px).
///
/// Order: every slot whose id left this frame lifts first (`MT_SLOT` + `MT_TRACKING_ID -1`).
/// Then every slot still occupied — continuing or brand new — gets `MT_SLOT` (+
/// `MT_TRACKING_ID` on a fresh assignment) and its position. A brand-new id takes the lowest
/// slot that was free *before* any lift this call ran, so a same-frame replacement (one id
/// leaves, another arrives) never reuses the slot it just vacated. Then, if anything above
/// was emitted, `BTN_TOUCH` and — only when the finger count changed — the old/new
/// `BTN_TOOL_*` bits. Finally, if a finger remains, `ABS_X`/`ABS_Y` of the lowest occupied
/// slot.
pub fn slot_events(state: &mut Slots, next: &[Contact], w: i32, h: i32) -> Vec<(u16, u16, i32)> {
    let next_ids: HashSet<u8> = next.iter().map(|&(id, _, _)| id).collect();

    // Absent from this frame → no longer locked out. If it comes back later it gets a fresh
    // shot at a slot.
    state.ignored.retain(|id| next_ids.contains(id));

    let old_count = state.slots.iter().filter(|s| s.is_some()).count();
    // Captured before any lift below runs: a slot a lift frees this call is not up for grabs
    // by a new id in the same call.
    let free_before: Vec<usize> = (0..SLOTS).filter(|&s| state.slots[s].is_none()).collect();

    let mut events = Vec::new();
    for s in 0..SLOTS {
        if let Some((id, _)) = state.slots[s]
            && !next_ids.contains(&id)
        {
            events.push((EV_ABS, ABS_MT_SLOT, s as i32));
            events.push((EV_ABS, ABS_MT_TRACKING_ID, -1));
            state.slots[s] = None;
        }
    }

    // (slot, x, y, Some(tracking_id) if this is a fresh assignment)
    let mut changed: Vec<(usize, u16, u16, Option<u16>)> = Vec::new();
    let mut free = free_before.into_iter();
    for &(id, x, y) in next {
        if state.ignored.contains(&id) {
            continue;
        }
        if let Some(slot) = (0..SLOTS).find(|&s| state.slots[s].is_some_and(|(sid, _)| sid == id)) {
            changed.push((slot, x, y, None));
        } else if let Some(slot) = free.next() {
            let tracking_id = state.next_tracking_id;
            state.next_tracking_id = state.next_tracking_id.wrapping_add(1);
            state.slots[slot] = Some((id, tracking_id));
            changed.push((slot, x, y, Some(tracking_id)));
        } else {
            state.ignored.insert(id);
        }
    }
    changed.sort_unstable_by_key(|e| e.0);

    for &(slot, x, y, tracking_id) in &changed {
        events.push((EV_ABS, ABS_MT_SLOT, slot as i32));
        if let Some(tracking_id) = tracking_id {
            events.push((EV_ABS, ABS_MT_TRACKING_ID, tracking_id as i32));
        }
        events.push((EV_ABS, ABS_MT_POSITION_X, scale(x, w)));
        events.push((EV_ABS, ABS_MT_POSITION_Y, scale(y, h)));
    }

    if events.is_empty() {
        return events;
    }

    let new_count = state.slots.iter().filter(|s| s.is_some()).count();
    events.push((EV_KEY, BTN_TOUCH, (new_count > 0) as i32));
    if old_count != new_count {
        if let Some(code) = tool_code(old_count) {
            events.push((EV_KEY, code, 0));
        }
        if let Some(code) = tool_code(new_count) {
            events.push((EV_KEY, code, 1));
        }
    }

    if let Some(lowest) = state.slots.iter().position(|s| s.is_some()) {
        let (id, _) = state.slots[lowest].expect("position() found a Some");
        if let Some(&(_, x, y)) = next.iter().find(|&&(nid, _, _)| nid == id) {
            events.push((EV_ABS, ABS_X, scale(x, w)));
            events.push((EV_ABS, ABS_Y, scale(y, h)));
        }
    }

    events
}

/// An open virtual touchpad. Dropping it destroys the kernel device.
pub struct Touchpad {
    device: uinput::Device,
    slots: Slots,
    w_px: u16,
    h_px: u16,
}

impl Touchpad {
    /// Create the device, sized to the phone's CSS px (`w_px` × `h_px`).
    pub fn new(w_px: u16, h_px: u16) -> io::Result<Touchpad> {
        let w = w_px as i32 * 10;
        let h = h_px as i32 * 10;
        // The axis `resolution` libinput divides motion by: the phone's CSS px per mm
        // (`trackpad { px-per-mm }`), ×10 for the ×10 axis range. Not measured per phone.
        let res = wado_config::live::current()
            .trackpad
            .px_per_mm
            .clamp(1, 100) as i32
            * 10;

        let device = uinput::Device::open("wado trackpad", |d| {
            d.evbit(EV_KEY)?;
            d.evbit(EV_ABS)?;
            d.propbit(INPUT_PROP_POINTER)?;
            d.propbit(INPUT_PROP_BUTTONPAD)?;
            for key in [
                BTN_LEFT,
                BTN_TOUCH,
                BTN_TOOL_FINGER,
                BTN_TOOL_DOUBLETAP,
                BTN_TOOL_TRIPLETAP,
                BTN_TOOL_QUADTAP,
                BTN_TOOL_QUINTTAP,
            ] {
                d.keybit(key)?;
            }
            d.absbit(ABS_X)?;
            d.abs(ABS_X, w, res)?;
            d.absbit(ABS_Y)?;
            d.abs(ABS_Y, h, res)?;
            d.absbit(ABS_MT_SLOT)?;
            d.abs(ABS_MT_SLOT, (SLOTS - 1) as i32, 0)?;
            d.absbit(ABS_MT_POSITION_X)?;
            d.abs(ABS_MT_POSITION_X, w, res)?;
            d.absbit(ABS_MT_POSITION_Y)?;
            d.abs(ABS_MT_POSITION_Y, h, res)?;
            d.absbit(ABS_MT_TRACKING_ID)?;
            d.abs(ABS_MT_TRACKING_ID, 65535, 0)?;
            Ok(())
        })?;

        Ok(Touchpad {
            device,
            slots: Slots::default(),
            w_px,
            h_px,
        })
    }

    /// Report one frame: every finger down right now, `[]` meaning all lifted.
    pub fn frame(&mut self, c: &[Contact]) -> io::Result<()> {
        let w = self.w_px as i32 * 10;
        let h = self.h_px as i32 * 10;
        let events = slot_events(&mut self.slots, c, w, h);
        if !events.is_empty() {
            self.device.emit(&events)?;
        }
        Ok(())
    }

    /// The pad's size in CSS px, as given to [`Self::new`].
    pub fn size(&self) -> (u16, u16) {
        (self.w_px, self.h_px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_finger_down_moves_then_lifts() {
        let mut slots = Slots::default();
        let down = slot_events(&mut slots, &[(0, 32768, 32768)], 3900, 8000);
        assert!(down.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 0)));
        assert!(down.contains(&(EV_KEY, BTN_TOUCH, 1)));
        assert!(down.contains(&(EV_KEY, BTN_TOOL_FINGER, 1)));
        assert!(down.contains(&(EV_ABS, ABS_MT_POSITION_X, 1950)));
        let up = slot_events(&mut slots, &[], 3900, 8000);
        assert!(up.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)));
        assert!(up.contains(&(EV_KEY, BTN_TOUCH, 0)));
        assert!(up.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)));
    }

    #[test]
    fn second_finger_switches_tool_to_doubletap() {
        let mut slots = Slots::default();
        slot_events(&mut slots, &[(0, 0, 0)], 100, 100);
        let e = slot_events(&mut slots, &[(0, 0, 0), (1, 10, 10)], 100, 100);
        assert!(e.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)));
        assert!(e.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 1)));
    }

    /// Adapted: a sixth finger gets no tracking id, this call or ever again, even though the
    /// other five keep refreshing their (unchanged) positions every frame.
    #[test]
    fn a_sixth_finger_is_ignored() {
        let mut slots = Slots::default();
        let five: Vec<Contact> = (0..5).map(|i| (i, 0, 0)).collect();
        slot_events(&mut slots, &five, 100, 100);
        let mut six = five.clone();
        six.push((5, 0, 0));
        let e = slot_events(&mut slots, &six, 100, 100);
        assert!(
            !e.iter()
                .any(|&(k, c, v)| k == EV_ABS && c == ABS_MT_TRACKING_ID && v >= 0)
        );
    }

    /// (a) The lower-numbered finger lifts; the other keeps its own slot (not slot 0 — the
    /// CRITICAL bug this replaces re-derived slots from list position every call, so the
    /// survivor's events silently landed on slot 0 with no tracking id). It then moves, still
    /// in its own slot, and finally lifts, releasing that slot.
    #[test]
    fn lower_finger_lifts_then_the_other_moves_then_lifts() {
        let mut slots = Slots::default();

        let e1 = slot_events(&mut slots, &[(0, 100, 100), (1, 30000, 30000)], 1000, 1000);
        assert_eq!(
            e1,
            vec![
                (EV_ABS, ABS_MT_SLOT, 0),
                (EV_ABS, ABS_MT_TRACKING_ID, 0),
                (EV_ABS, ABS_MT_POSITION_X, scale(100, 1000)),
                (EV_ABS, ABS_MT_POSITION_Y, scale(100, 1000)),
                (EV_ABS, ABS_MT_SLOT, 1),
                (EV_ABS, ABS_MT_TRACKING_ID, 1),
                (EV_ABS, ABS_MT_POSITION_X, scale(30000, 1000)),
                (EV_ABS, ABS_MT_POSITION_Y, scale(30000, 1000)),
                (EV_KEY, BTN_TOUCH, 1),
                (EV_KEY, BTN_TOOL_DOUBLETAP, 1),
                (EV_ABS, ABS_X, scale(100, 1000)),
                (EV_ABS, ABS_Y, scale(100, 1000)),
            ]
        );

        let e2 = slot_events(&mut slots, &[(1, 30000, 30000)], 1000, 1000);
        assert_eq!(
            e2,
            vec![
                (EV_ABS, ABS_MT_SLOT, 0),
                (EV_ABS, ABS_MT_TRACKING_ID, -1),
                (EV_ABS, ABS_MT_SLOT, 1),
                (EV_ABS, ABS_MT_POSITION_X, scale(30000, 1000)),
                (EV_ABS, ABS_MT_POSITION_Y, scale(30000, 1000)),
                (EV_KEY, BTN_TOUCH, 1),
                (EV_KEY, BTN_TOOL_DOUBLETAP, 0),
                (EV_KEY, BTN_TOOL_FINGER, 1),
                (EV_ABS, ABS_X, scale(30000, 1000)),
                (EV_ABS, ABS_Y, scale(30000, 1000)),
            ],
            "the survivor's position must land on its own slot 1, not slot 0"
        );

        let e3 = slot_events(&mut slots, &[(1, 31000, 31000)], 1000, 1000);
        assert_eq!(
            e3,
            vec![
                (EV_ABS, ABS_MT_SLOT, 1),
                (EV_ABS, ABS_MT_POSITION_X, scale(31000, 1000)),
                (EV_ABS, ABS_MT_POSITION_Y, scale(31000, 1000)),
                (EV_KEY, BTN_TOUCH, 1),
                (EV_ABS, ABS_X, scale(31000, 1000)),
                (EV_ABS, ABS_Y, scale(31000, 1000)),
            ],
            "still slot 1 — moving must not reassign the slot"
        );

        let e4 = slot_events(&mut slots, &[], 1000, 1000);
        assert_eq!(
            e4,
            vec![
                (EV_ABS, ABS_MT_SLOT, 1),
                (EV_ABS, ABS_MT_TRACKING_ID, -1),
                (EV_KEY, BTN_TOUCH, 0),
                (EV_KEY, BTN_TOOL_FINGER, 0),
            ],
            "the final lift must release slot 1, not a stale slot 0"
        );
    }

    /// (b) A same-frame replacement: id 0 lifts and id 1 appears in the same call. The
    /// IMPORTANT bug this replaces reused slot 0 for id 1 with no `-1` in between; the new
    /// contact must land on a different slot, with a fresh tracking id, and the finger count
    /// must not blip to 2.
    #[test]
    fn replacement_in_one_frame_frees_then_reassigns() {
        let mut slots = Slots::default();
        let e1 = slot_events(&mut slots, &[(0, 100, 100)], 1000, 1000);
        assert_eq!(
            e1,
            vec![
                (EV_ABS, ABS_MT_SLOT, 0),
                (EV_ABS, ABS_MT_TRACKING_ID, 0),
                (EV_ABS, ABS_MT_POSITION_X, scale(100, 1000)),
                (EV_ABS, ABS_MT_POSITION_Y, scale(100, 1000)),
                (EV_KEY, BTN_TOUCH, 1),
                (EV_KEY, BTN_TOOL_FINGER, 1),
                (EV_ABS, ABS_X, scale(100, 1000)),
                (EV_ABS, ABS_Y, scale(100, 1000)),
            ]
        );

        let e2 = slot_events(&mut slots, &[(1, 30000, 30000)], 1000, 1000);
        assert_eq!(
            e2,
            vec![
                (EV_ABS, ABS_MT_SLOT, 0),
                (EV_ABS, ABS_MT_TRACKING_ID, -1),
                (EV_ABS, ABS_MT_SLOT, 1),
                (EV_ABS, ABS_MT_TRACKING_ID, 1),
                (EV_ABS, ABS_MT_POSITION_X, scale(30000, 1000)),
                (EV_ABS, ABS_MT_POSITION_Y, scale(30000, 1000)),
                (EV_KEY, BTN_TOUCH, 1),
                (EV_ABS, ABS_X, scale(30000, 1000)),
                (EV_ABS, ABS_Y, scale(30000, 1000)),
            ],
            "id 1 must land on slot 1 (never the just-freed slot 0) with a fresh tracking id, \
             and the finger count must stay 1 throughout"
        );
    }

    /// (c) Six fingers at once: the sixth is locked out immediately. Lifting the first finger
    /// frees slot 0, but the sixth must stay locked out — not promoted into the freed slot —
    /// and a later move of the sixth must produce no event for it at all.
    #[test]
    fn sixth_finger_stays_ignored_even_after_a_slot_frees() {
        let mut slots = Slots::default();
        let six: Vec<Contact> = (0..6).map(|i| (i, 10, 10)).collect();
        let e1 = slot_events(&mut slots, &six, 1000, 1000);
        let appeared = e1
            .iter()
            .filter(|&&(k, c, v)| k == EV_ABS && c == ABS_MT_TRACKING_ID && v >= 0)
            .count();
        assert_eq!(
            appeared, 5,
            "only 5 slots exist — the 6th id gets no tracking id"
        );

        let drop_zero: Vec<Contact> = (1..6).map(|i| (i, 10, 10)).collect();
        let e2 = slot_events(&mut slots, &drop_zero, 1000, 1000);
        assert!(e2.contains(&(EV_ABS, ABS_MT_SLOT, 0)));
        assert!(e2.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)));
        let appeared2 = e2
            .iter()
            .filter(|&&(k, c, v)| k == EV_ABS && c == ABS_MT_TRACKING_ID && v >= 0)
            .count();
        assert_eq!(
            appeared2, 0,
            "id 5 must not be promoted into the freed slot 0"
        );

        let moved: Vec<Contact> = (1..6).map(|i| (i, 20000, 20000)).collect();
        let e3 = slot_events(&mut slots, &moved, 1000, 1000);
        let slot_updates = e3
            .iter()
            .filter(|&&(k, c, _)| k == EV_ABS && c == ABS_MT_SLOT)
            .count();
        assert_eq!(
            slot_updates, 4,
            "id 5 is still ignored — only 4 real slots move"
        );
    }

    /// (d) Tracking ids come from a counter, never the client id: lifting and re-pressing the
    /// same client id must hand out a new, larger tracking id.
    #[test]
    fn tracking_ids_increase_across_lift_and_repress() {
        let mut slots = Slots::default();
        let find_trk = |events: &[(u16, u16, i32)]| {
            events
                .iter()
                .find(|&&(k, c, _)| k == EV_ABS && c == ABS_MT_TRACKING_ID)
                .map(|&(_, _, v)| v)
                .unwrap()
        };

        let first = slot_events(&mut slots, &[(0, 0, 0)], 100, 100);
        let trk1 = find_trk(&first);

        slot_events(&mut slots, &[], 100, 100);
        let second = slot_events(&mut slots, &[(0, 0, 0)], 100, 100);
        let trk2 = find_trk(&second);

        assert!(
            trk2 > trk1,
            "re-press must get a larger tracking id ({trk1} -> {trk2})"
        );
    }

    #[test]
    #[ignore = "needs /dev/uinput"]
    fn device_is_created_and_destroyed() {
        let mut t = Touchpad::new(390, 800).unwrap();
        t.frame(&[(0, 100, 100)]).unwrap();
        t.frame(&[]).unwrap();
    }
}
