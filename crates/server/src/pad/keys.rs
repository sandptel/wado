//! The virtual keyboard: US-layout key/char lookup, chord construction, and the device that
//! tracks what it currently holds so a lifted connection cannot leave a modifier stuck down.

use std::{collections::HashSet, io};

use wado_protocol::pad::Mod;

use super::uinput::{self, EV_KEY};

/// `KEY_*` codes for the named keys `PadMsg::Key` carries (kernel header `linux/input-event-codes.h`).
pub fn key_code(name: &str) -> Option<u16> {
    Some(match name {
        "esc" => 1,
        "tab" => 15,
        "enter" => 28,
        "backspace" => 14,
        "delete" => 111,
        "left" => 105,
        "right" => 106,
        "up" => 103,
        "down" => 108,
        "home" => 102,
        "end" => 107,
        "pageup" => 104,
        "pagedown" => 109,
        "space" => 57,
        "f1" => 59,
        "f2" => 60,
        "f3" => 61,
        "f4" => 62,
        "f5" => 63,
        "f6" => 64,
        "f7" => 65,
        "f8" => 66,
        "f9" => 67,
        "f10" => 68,
        "f11" => 87,
        "f12" => 88,
        _ => return None,
    })
}

/// `KEY_*` code for one modifier.
pub fn mod_code(m: Mod) -> u16 {
    match m {
        Mod::Ctrl => 29,
        Mod::Alt => 56,
        Mod::Shift => 42,
        Mod::Super => 125,
    }
}

/// ponytail: US table only — non-US hosts get the wrong punctuation out of `char_code`; the
/// upgrade is an xkbcommon lookup of the host's actual keymap, not a bigger match arm here.
pub fn char_code(ch: char) -> Option<(u16, bool)> {
    let (code, shift) = match ch {
        'a'..='z' => (letter_code(ch), false),
        'A'..='Z' => (letter_code(ch.to_ascii_lowercase()), true),
        '1' => (2, false),
        '2' => (3, false),
        '3' => (4, false),
        '4' => (5, false),
        '5' => (6, false),
        '6' => (7, false),
        '7' => (8, false),
        '8' => (9, false),
        '9' => (10, false),
        '0' => (11, false),
        '!' => (2, true),
        '@' => (3, true),
        '#' => (4, true),
        '$' => (5, true),
        '%' => (6, true),
        '^' => (7, true),
        '&' => (8, true),
        '*' => (9, true),
        '(' => (10, true),
        ')' => (11, true),
        ' ' => (57, false),
        '\n' => (28, false),
        '\t' => (15, false),
        '-' => (12, false),
        '_' => (12, true),
        '=' => (13, false),
        '+' => (13, true),
        '[' => (26, false),
        '{' => (26, true),
        ']' => (27, false),
        '}' => (27, true),
        '\\' => (43, false),
        '|' => (43, true),
        ';' => (39, false),
        ':' => (39, true),
        '\'' => (40, false),
        '"' => (40, true),
        '`' => (41, false),
        '~' => (41, true),
        ',' => (51, false),
        '<' => (51, true),
        '.' => (52, false),
        '>' => (52, true),
        '/' => (53, false),
        '?' => (53, true),
        _ => return None,
    };
    Some((code, shift))
}

/// `KEY_*` code for one lowercase `a..=z`, laid out as the US QWERTY rows.
fn letter_code(ch: char) -> u16 {
    match ch {
        'q' => 16,
        'w' => 17,
        'e' => 18,
        'r' => 19,
        't' => 20,
        'y' => 21,
        'u' => 22,
        'i' => 23,
        'o' => 24,
        'p' => 25,
        'a' => 30,
        's' => 31,
        'd' => 32,
        'f' => 33,
        'g' => 34,
        'h' => 35,
        'j' => 36,
        'k' => 37,
        'l' => 38,
        'z' => 44,
        'x' => 45,
        'c' => 46,
        'v' => 47,
        'b' => 48,
        'n' => 49,
        'm' => 50,
        _ => unreachable!("letter_code called with a non a..=z char"),
    }
}

/// The press/release sequence for one key tapped under `mods`: mods down in the given order,
/// the key down then up, mods up in reverse order.
pub fn chord_events(mods: &[Mod], code: u16) -> Vec<(u16, i32)> {
    let mut events = Vec::with_capacity(mods.len() * 2 + 2);
    for &m in mods {
        events.push((mod_code(m), 1));
    }
    events.push((code, 1));
    events.push((code, 0));
    for &m in mods.iter().rev() {
        events.push((mod_code(m), 0));
    }
    events
}

/// An open virtual keyboard. Tracks which codes it currently holds (from [`Self::hold`]) so
/// `Drop` can release them — a dropped connection must never leave a modifier stuck down on
/// the host.
/// `mods` without the ones whose key is already in `held`, order kept.
fn unheld(mods: &[Mod], held: &HashSet<u16>) -> Vec<Mod> {
    mods.iter()
        .copied()
        .filter(|&m| !held.contains(&mod_code(m)))
        .collect()
}

/// Move `held` to `target` one key event at a time through `emit`: keys no longer wanted are
/// released, then keys newly wanted pressed. `held` follows each event that succeeded, so a
/// failure partway leaves it saying what is really down.
fn hold_into(
    held: &mut HashSet<u16>,
    target: &HashSet<u16>,
    mut emit: impl FnMut(u16, i32) -> io::Result<()>,
) -> io::Result<()> {
    for code in held.difference(target).copied().collect::<Vec<_>>() {
        emit(code, 0)?;
        held.remove(&code);
    }
    for code in target.difference(held).copied().collect::<Vec<_>>() {
        emit(code, 1)?;
        held.insert(code);
    }
    Ok(())
}

pub struct Keyboard {
    device: uinput::Device,
    held: HashSet<u16>,
}

impl Keyboard {
    /// Create the device, declaring every `KEY_*`/`BTN_*` bit in `1..=255`.
    pub fn new() -> io::Result<Keyboard> {
        let device = uinput::Device::open("wado keyboard", |d| {
            d.evbit(EV_KEY)?;
            for code in 1..=255u16 {
                d.keybit(code)?;
            }
            Ok(())
        })?;
        Ok(Keyboard {
            device,
            held: HashSet::new(),
        })
    }

    /// Tap one key under `mods`: press mods, press/release the key, release mods. Does not
    /// touch [`Self::held`] — a tap never outlives this call.
    pub fn tap(&mut self, mods: &[Mod], code: u16) -> io::Result<()> {
        // A modifier already held (a locked one) stays down: pressing it again would be a no-op,
        // and releasing it at the end of the chord would unlock it behind the user's back.
        let mods = unheld(mods, &self.held);
        for &(c, v) in &chord_events(&mods, code) {
            self.device.emit(&[(EV_KEY, c, v)])?;
        }
        Ok(())
    }

    /// Hold exactly `mods` down, diffed against whatever is currently held: codes no longer
    /// present are released, codes newly present are pressed. `[]` releases everything.
    pub fn hold(&mut self, mods: &[Mod]) -> io::Result<()> {
        let target: HashSet<u16> = mods.iter().map(|&m| mod_code(m)).collect();
        let device = &self.device;
        hold_into(&mut self.held, &target, |code, v| {
            device.emit(&[(EV_KEY, code, v)])
        })
    }

    /// Release every held key.
    pub fn release_all(&mut self) -> io::Result<()> {
        self.hold(&[])
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        let _ = self.release_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_table() {
        assert_eq!(char_code('a'), Some((30, false)));
        assert_eq!(char_code('A'), Some((30, true)));
        assert_eq!(char_code('1'), Some((2, false)));
        assert_eq!(char_code('!'), Some((2, true)));
        assert_eq!(char_code(' '), Some((57, false)));
        assert_eq!(char_code('é'), None);
        assert_eq!(key_code("esc"), Some(1));
        assert_eq!(key_code("nope"), None);
    }

    #[test]
    fn chord_presses_mods_around_the_key() {
        assert_eq!(
            chord_events(&[Mod::Super], 2),
            vec![(125, 1), (2, 1), (2, 0), (125, 0)]
        );
        assert_eq!(
            chord_events(&[Mod::Ctrl, Mod::Shift], 30),
            vec![(29, 1), (42, 1), (30, 1), (30, 0), (42, 0), (29, 0)]
        );
    }

    #[test]
    fn a_held_modifier_is_left_alone_by_a_tap() {
        let held: HashSet<u16> = [mod_code(Mod::Super)].into();
        assert_eq!(unheld(&[Mod::Super, Mod::Shift], &held), vec![Mod::Shift]);
        assert_eq!(unheld(&[Mod::Ctrl], &held), vec![Mod::Ctrl]);
    }

    #[test]
    fn a_failed_hold_leaves_held_truthful() {
        let (ctrl, alt, shift) = (
            mod_code(Mod::Ctrl),
            mod_code(Mod::Alt),
            mod_code(Mod::Shift),
        );
        let mut held: HashSet<u16> = [ctrl].into();
        let target: HashSet<u16> = [alt, shift].into();
        // the release of Ctrl and the first press succeed; the second press fails
        let mut ok = 2;
        let r = hold_into(&mut held, &target, |_, _| {
            if ok == 0 {
                return Err(io::Error::other("device gone"));
            }
            ok -= 1;
            Ok(())
        });
        assert!(r.is_err());
        assert!(!held.contains(&ctrl), "Ctrl was released");
        assert_eq!(
            held.len(),
            1,
            "exactly one of Alt/Shift went down: {held:?}"
        );

        // a release that fails keeps the key counted as held
        let mut held: HashSet<u16> = [ctrl].into();
        let r = hold_into(&mut held, &HashSet::new(), |_, _| {
            Err(io::Error::other("device gone"))
        });
        assert!(r.is_err());
        assert_eq!(held, HashSet::from([ctrl]));
    }

    #[test]
    #[ignore = "needs /dev/uinput"]
    fn device_is_created_and_destroyed() {
        let mut k = Keyboard::new().unwrap();
        k.tap(&[Mod::Shift], 30).unwrap();
        k.hold(&[Mod::Ctrl]).unwrap();
        k.release_all().unwrap();
    }
}
