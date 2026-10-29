//! The virtual multi-touch touchpad: device setup, and the pure logic that turns one frame of
//! [`Contact`]s into evdev slot events.

use std::io;

use wado_protocol::pad::Contact;

use super::uinput::{self, EV_ABS, EV_KEY};

/// A phone's CSS px per mm, roughly — the axis `resolution` libinput divides pointer motion
/// by. ponytail: this is the one knob to turn if pointer speed on the host feels off; it is
/// not measured per-device, just a reasonable constant for a phone-sized touch surface.
const CSS_PX_PER_MM: i32 = 6;

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

/// Assign each contact's id a slot (0..=4), keeping a continuing id's previous slot and giving
/// a new id the first slot not already in use. An id past the slot budget keeps no slot (the
/// touch is dropped from `events`, but not from the device: nothing moves because nothing is
/// mapped).
fn assign_slots(prev: &[Contact], next: &[Contact]) -> std::collections::HashMap<u8, u16> {
    let mut prev_slots = std::collections::HashMap::new();
    for &(id, _, _) in prev.iter().take(SLOTS) {
        let slot = prev_slots.len() as u16;
        prev_slots.entry(id).or_insert(slot);
    }

    let mut next_slots = std::collections::HashMap::new();
    let mut used = [false; SLOTS];
    // Continuing ids keep their slot first, so a new id never bumps one still touching.
    for &(id, _, _) in next {
        if let Some(&s) = prev_slots.get(&id) {
            next_slots.insert(id, s);
            used[s as usize] = true;
        }
    }
    for &(id, _, _) in next {
        if next_slots.contains_key(&id) {
            continue;
        }
        if let Some(free) = (0..SLOTS).find(|&s| !used[s]) {
            next_slots.insert(id, free as u16);
            used[free] = true;
        }
        // else: no free slot — this finger is silently ignored.
    }
    next_slots
}

/// The evdev events that take the device from `prev`'s contacts to `next`'s, pure and
/// stateless. `w`/`h` are the device's axis maxima (already the `*10` device range, not CSS
/// px).
pub fn slot_events(prev: &[Contact], next: &[Contact], w: i32, h: i32) -> Vec<(u16, u16, i32)> {
    let prev_slots = assign_slots(&[], prev); // prev's own slot assignment, by its own order
    let next_slots = assign_slots(prev, next);

    let prev_used: std::collections::HashSet<u16> = prev_slots.values().copied().collect();
    let next_used: std::collections::HashSet<u16> = next_slots.values().copied().collect();

    let mut lifted: Vec<u16> = prev_used.difference(&next_used).copied().collect();
    lifted.sort_unstable();

    // For each slot now occupied, whether it's a brand-new id (appeared) or a continuing one
    // that may have moved.
    let mut changed: Vec<(u16, u8, u16, u16, bool)> = Vec::new(); // (slot, id, x, y, appeared)
    for &(id, x, y) in next {
        let Some(&slot) = next_slots.get(&id) else {
            continue;
        };
        match prev.iter().find(|&&(pid, _, _)| pid == id) {
            None => changed.push((slot, id, x, y, true)),
            Some(&(_, px, py)) if (px, py) != (x, y) => changed.push((slot, id, x, y, false)),
            Some(_) => {}
        }
    }
    changed.sort_unstable_by_key(|e| e.0);

    let mut events = Vec::new();
    for &slot in &lifted {
        events.push((EV_ABS, ABS_MT_SLOT, slot as i32));
        events.push((EV_ABS, ABS_MT_TRACKING_ID, -1));
    }
    for &(slot, id, x, y, appeared) in &changed {
        events.push((EV_ABS, ABS_MT_SLOT, slot as i32));
        if appeared {
            events.push((EV_ABS, ABS_MT_TRACKING_ID, id as i32));
        }
        events.push((EV_ABS, ABS_MT_POSITION_X, scale(x, w)));
        events.push((EV_ABS, ABS_MT_POSITION_Y, scale(y, h)));
    }

    if events.is_empty() {
        return events;
    }

    let old_count = prev_used.len();
    let new_count = next_used.len();
    events.push((EV_KEY, BTN_TOUCH, (new_count > 0) as i32));
    if old_count != new_count {
        if let Some(code) = tool_code(old_count) {
            events.push((EV_KEY, code, 0));
        }
        if let Some(code) = tool_code(new_count) {
            events.push((EV_KEY, code, 1));
        }
    }

    if let Some((&id, _)) = next_slots.iter().min_by_key(|&(_, &s)| s) {
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
    prev: Vec<Contact>,
    w_px: u16,
    h_px: u16,
}

impl Touchpad {
    /// Create the device, sized to the phone's CSS px (`w_px` × `h_px`).
    pub fn new(w_px: u16, h_px: u16) -> io::Result<Touchpad> {
        let w = w_px as i32 * 10;
        let h = h_px as i32 * 10;
        let res = CSS_PX_PER_MM * 10;

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
            prev: Vec::new(),
            w_px,
            h_px,
        })
    }

    /// Report one frame: every finger down right now, `[]` meaning all lifted.
    pub fn frame(&mut self, c: &[Contact]) -> io::Result<()> {
        let w = self.w_px as i32 * 10;
        let h = self.h_px as i32 * 10;
        let events = slot_events(&self.prev, c, w, h);
        if !events.is_empty() {
            self.device.emit(&events)?;
        }
        self.prev = c.to_vec();
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
        let down = slot_events(&[], &[(0, 32768, 32768)], 3900, 8000);
        assert!(down.contains(&(EV_ABS, ABS_MT_TRACKING_ID, 0)));
        assert!(down.contains(&(EV_KEY, BTN_TOUCH, 1)));
        assert!(down.contains(&(EV_KEY, BTN_TOOL_FINGER, 1)));
        assert!(down.contains(&(EV_ABS, ABS_MT_POSITION_X, 1950)));
        let up = slot_events(&[(0, 32768, 32768)], &[], 3900, 8000);
        assert!(up.contains(&(EV_ABS, ABS_MT_TRACKING_ID, -1)));
        assert!(up.contains(&(EV_KEY, BTN_TOUCH, 0)));
        assert!(up.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)));
    }

    #[test]
    fn second_finger_switches_tool_to_doubletap() {
        let e = slot_events(&[(0, 0, 0)], &[(0, 0, 0), (1, 10, 10)], 100, 100);
        assert!(e.contains(&(EV_KEY, BTN_TOOL_FINGER, 0)));
        assert!(e.contains(&(EV_KEY, BTN_TOOL_DOUBLETAP, 1)));
    }

    #[test]
    fn a_sixth_finger_is_ignored() {
        let five: Vec<Contact> = (0..5).map(|i| (i, 0, 0)).collect();
        let mut six = five.clone();
        six.push((5, 0, 0));
        assert!(slot_events(&five, &six, 100, 100).is_empty());
    }

    #[test]
    #[ignore = "needs /dev/uinput"]
    fn device_is_created_and_destroyed() {
        let mut t = Touchpad::new(390, 800).unwrap();
        t.frame(&[(0, 100, 100)]).unwrap();
        t.frame(&[]).unwrap();
    }
}
