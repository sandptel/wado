//! The virtual keyboard ([`KeyInjector`] on Linux): chord construction, and a uinput device that
//! tracks what it currently holds so a lifted connection cannot leave a modifier stuck down.
//!
//! A tap's whole chord — mods down, key down and up, mods up, each its own report — goes to the
//! kernel in one `write(2)`.

use std::{collections::HashSet, io};

use wado_platform::input::{InputRes, KeyInjector};
use wado_protocol::pad::Mod;

use super::uinput::{self, EV_KEY};
use super::why;

/// `KEY_*` code for one modifier (kernel header `linux/input-event-codes.h`).
pub fn mod_code(m: Mod) -> u16 {
    match m {
        Mod::Ctrl => 29,
        Mod::Alt => 56,
        Mod::Shift => 42,
        Mod::Super => 125,
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

/// An open virtual keyboard. Tracks which codes it currently holds (from [`Self::hold`]) so
/// `Drop` can release them — a dropped connection must never leave a modifier stuck down on
/// the host.
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
        let chord: Vec<(u16, u16, i32)> = chord_events(&mods, code)
            .into_iter()
            .map(|(c, v)| (EV_KEY, c, v))
            .collect();
        self.device.emit_each(&chord)
    }

    /// Hold exactly `mods` down, diffed against whatever is currently held: codes no longer
    /// present are released, codes newly present are pressed. `[]` releases everything.
    pub fn hold(&mut self, mods: &[Mod]) -> io::Result<()> {
        let target: HashSet<u16> = mods.iter().map(|&m| mod_code(m)).collect();
        let device = &mut self.device;
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

impl KeyInjector for Keyboard {
    fn tap(&mut self, mods: &[Mod], code: u16) -> InputRes {
        Keyboard::tap(self, mods, code).map_err(why)
    }
    fn hold(&mut self, mods: &[Mod]) -> InputRes {
        Keyboard::hold(self, mods).map_err(why)
    }
    fn release_all(&mut self) -> InputRes {
        Keyboard::release_all(self).map_err(why)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The platform contract, on the real device.
    #[test]
    #[ignore = "needs /dev/uinput"]
    fn passes_the_key_contract() {
        let open = || {
            Keyboard::new()
                .map(|k| Box::new(k) as Box<dyn KeyInjector>)
                .map_err(why)
        };
        wado_platform::fake::contract::keys(&open);
    }
}
