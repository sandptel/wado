//! The virtual multi-touch touchpad ([`TouchInjector`] on Linux): device setup, and the logic
//! that turns one frame of [`Contact`]s into evdev slot events — without allocating, and written
//! to the kernel in one `write(2)`, since this runs for every frame of every finger.

use std::io;

use wado_platform::input::{InputRes, TouchInjector};
use wado_protocol::pad::Contact;

use super::uinput::{self, EV_ABS, EV_KEY};
use super::why;

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

/// A set of client finger ids (`u8`), as a 256-bit bitmap — no allocation per frame.
#[derive(Default, Clone, Copy)]
struct IdSet([u64; 4]);

impl IdSet {
    fn insert(&mut self, id: u8) {
        self.0[id as usize >> 6] |= 1 << (id & 63);
    }
    fn contains(&self, id: u8) -> bool {
        self.0[id as usize >> 6] & (1 << (id & 63)) != 0
    }
    /// Keep only the ids also in `other`.
    fn keep(&mut self, other: &IdSet) {
        for (a, b) in self.0.iter_mut().zip(other.0) {
            *a &= b;
        }
    }
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
    ignored: IdSet,
}

/// The evdev events that take the device from `state`'s current slot assignment to `next`'s,
/// written into `events` (cleared first, so one buffer serves every frame), mutating `state` in
/// place. `w`/`h` are the device's axis maxima (already the `*10` device range, not CSS px).
///
/// Order: every slot whose id left this frame lifts first (`MT_SLOT` + `MT_TRACKING_ID -1`).
/// Then every slot still occupied — continuing or brand new — gets `MT_SLOT` (+
/// `MT_TRACKING_ID` on a fresh assignment) and its position. A brand-new id takes the lowest
/// slot that was free *before* any lift this call ran, so a same-frame replacement (one id
/// leaves, another arrives) never reuses the slot it just vacated. Then, if anything above
/// was emitted, `BTN_TOUCH` and — only when the finger count changed — the old/new
/// `BTN_TOOL_*` bits. Finally, if a finger remains, `ABS_X`/`ABS_Y` of the lowest occupied
/// slot. An id named twice in `next` counts once, at its first position (the pad already
/// drops repeats).
pub fn slot_events(
    state: &mut Slots,
    next: &[Contact],
    w: i32,
    h: i32,
    events: &mut Vec<(u16, u16, i32)>,
) {
    events.clear();
    let mut next_ids = IdSet::default();
    for &(id, _, _) in next {
        next_ids.insert(id);
    }

    // Absent from this frame → no longer locked out. If it comes back later it gets a fresh
    // shot at a slot.
    state.ignored.keep(&next_ids);

    let old_count = state.slots.iter().filter(|s| s.is_some()).count();
    // Captured before any lift below runs: a slot a lift frees this call is not up for grabs
    // by a new id in the same call.
    let free_before = state.slots.map(|s| s.is_none());

    for s in 0..SLOTS {
        if let Some((id, _)) = state.slots[s]
            && !next_ids.contains(id)
        {
            events.push((EV_ABS, ABS_MT_SLOT, s as i32));
            events.push((EV_ABS, ABS_MT_TRACKING_ID, -1));
            state.slots[s] = None;
        }
    }

    // Per slot: (x, y, Some(tracking_id) if this is a fresh assignment).
    let mut changed: [Option<(u16, u16, Option<u16>)>; SLOTS] = [None; SLOTS];
    let mut free = (0..SLOTS).filter(|&s| free_before[s]);
    for &(id, x, y) in next {
        if state.ignored.contains(id) {
            continue;
        }
        if let Some(slot) = (0..SLOTS).find(|&s| state.slots[s].is_some_and(|(sid, _)| sid == id)) {
            changed[slot].get_or_insert((x, y, None));
        } else if let Some(slot) = free.next() {
            let tracking_id = state.next_tracking_id;
            state.next_tracking_id = state.next_tracking_id.wrapping_add(1);
            state.slots[slot] = Some((id, tracking_id));
            changed[slot] = Some((x, y, Some(tracking_id)));
        } else {
            state.ignored.insert(id);
        }
    }

    for (slot, c) in changed.iter().enumerate() {
        let Some((x, y, tracking_id)) = *c else {
            continue;
        };
        events.push((EV_ABS, ABS_MT_SLOT, slot as i32));
        if let Some(tracking_id) = tracking_id {
            events.push((EV_ABS, ABS_MT_TRACKING_ID, tracking_id as i32));
        }
        events.push((EV_ABS, ABS_MT_POSITION_X, scale(x, w)));
        events.push((EV_ABS, ABS_MT_POSITION_Y, scale(y, h)));
    }

    if events.is_empty() {
        return;
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

    // Every slot still occupied was named in `next`, so it has a position this frame.
    if let Some((x, y, _)) = state
        .slots
        .iter()
        .position(|s| s.is_some())
        .and_then(|lowest| changed[lowest])
    {
        events.push((EV_ABS, ABS_X, scale(x, w)));
        events.push((EV_ABS, ABS_Y, scale(y, h)));
    }
}

/// An open virtual touchpad. Dropping it destroys the kernel device.
pub struct Touchpad {
    device: uinput::Device,
    slots: Slots,
    /// One frame's events, reused for every frame.
    events: Vec<(u16, u16, i32)>,
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
            events: Vec::with_capacity(4 * SLOTS + 6),
            w_px,
            h_px,
        })
    }

    /// Report one frame: every finger down right now, `[]` meaning all lifted. One `write(2)`.
    pub fn frame(&mut self, c: &[Contact]) -> io::Result<()> {
        let w = self.w_px as i32 * 10;
        let h = self.h_px as i32 * 10;
        slot_events(&mut self.slots, c, w, h, &mut self.events);
        if !self.events.is_empty() {
            self.device.emit(&self.events)?;
        }
        Ok(())
    }

    /// The pad's size in CSS px, as given to [`Self::new`].
    pub fn size(&self) -> (u16, u16) {
        (self.w_px, self.h_px)
    }
}

/// The platform's touchpad on Linux: the kernel device, made when the pad's size is first known
/// and made again when it changes — a uinput device's axes are fixed when it is created.
#[derive(Default)]
pub struct UinputTouch {
    pad: Option<Touchpad>,
}

impl TouchInjector for UinputTouch {
    fn resize(&mut self, w: u16, h: u16) -> InputRes {
        if self.pad.as_ref().is_some_and(|t| t.size() == (w, h)) {
            return Ok(());
        }
        self.pad = None; // the old device goes before the new one appears
        self.pad = Some(Touchpad::new(w, h).map_err(why)?);
        Ok(())
    }

    fn frame(&mut self, contacts: &[Contact]) -> InputRes {
        match self.pad.as_mut() {
            Some(pad) => pad.frame(contacts).map_err(why),
            None => Ok(()), // no size yet, so nothing can be down
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    /// The frame's events as a fresh vector — the tests' view of [`super::slot_events`].
    fn slot_events(state: &mut Slots, next: &[Contact], w: i32, h: i32) -> Vec<(u16, u16, i32)> {
        let mut out = Vec::new();
        super::slot_events(state, next, w, h, &mut out);
        out
    }

    /// The allocating implementation this replaced (HashSets, a sorted Vec per frame), kept as
    /// the oracle the bitmap version must match event for event.
    /// Persistent slot state, carried by the caller across frames (`Touchpad` owns one). A slot
    /// holds the client-assigned id currently occupying it and the kernel tracking id handed out
    /// for it; `next_tracking_id` is a counter, never the client id, because client ids are
    /// reused across lift/re-press while a tracking id must not be (`ABS_MT_TRACKING_ID` is how
    /// libinput tells a re-press apart from the same finger never having moved). `ignored` holds
    /// client ids that showed up with no free slot — they stay locked out for their whole
    /// lifetime, even once a slot frees, until they are absent from a frame at all.
    #[derive(Default)]
    struct RefSlots {
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
    fn reference(state: &mut RefSlots, next: &[Contact], w: i32, h: i32) -> Vec<(u16, u16, i32)> {
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
            if let Some((id, _)) = state.slots[s] {
                if !next_ids.contains(&id) {
                    events.push((EV_ABS, ABS_MT_SLOT, s as i32));
                    events.push((EV_ABS, ABS_MT_TRACKING_ID, -1));
                    state.slots[s] = None;
                }
            }
        }

        // (slot, x, y, Some(tracking_id) if this is a fresh assignment)
        let mut changed: Vec<(usize, u16, u16, Option<u16>)> = Vec::new();
        let mut free = free_before.into_iter();
        for &(id, x, y) in next {
            if state.ignored.contains(&id) {
                continue;
            }
            if let Some(slot) =
                (0..SLOTS).find(|&s| state.slots[s].is_some_and(|(sid, _)| sid == id))
            {
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

    /// Random frames — fingers landing, moving, lifting, more than five at once — produce the
    /// very same events from both implementations.
    #[test]
    fn matches_the_reference_implementation() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rnd = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let (mut a, mut b) = (Slots::default(), RefSlots::default());
        let mut out = Vec::new();
        for _ in 0..20_000 {
            let fingers = rnd(8) as usize;
            let mut seen = HashSet::new();
            let frame: Vec<Contact> = (0..fingers)
                .map(|_| (rnd(9) as u8, rnd(65536) as u16, rnd(65536) as u16))
                .filter(|c| seen.insert(c.0)) // the pad drops repeated ids first
                .collect();
            super::slot_events(&mut a, &frame, 3900, 8000, &mut out);
            assert_eq!(out, reference(&mut b, &frame, 3900, 8000), "{frame:?}");
        }
    }

    /// One frame's cost, by hand:
    /// `cargo test -p wado --release slot_events_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn slot_events_bench() {
        let mut slots = Slots::default();
        let mut out = Vec::new();
        let n = 5_000_000u32;
        let t = std::time::Instant::now();
        for i in 0..n {
            let x = (i % 60000) as u16;
            let c = [(0u8, x, x), (1u8, x / 2, 40000)];
            let next: &[Contact] = if i % 50 == 49 { &[] } else { &c };
            super::slot_events(&mut slots, next, 3900, 8000, &mut out);
            std::hint::black_box(&out);
        }
        eprintln!(
            "slot_events: {:.1} ns/frame",
            t.elapsed().as_nanos() as f64 / n as f64
        );
    }

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
