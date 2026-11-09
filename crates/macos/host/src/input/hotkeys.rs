//! The Mac's own shortcuts for Mission Control, App Exposé and the Spaces, as this Mac has them
//! set (System Settings › Keyboard › Shortcuts). Read through a private CoreGraphics call looked
//! up at run time; when it is missing or a shortcut is off, the macOS defaults are used.

use super::post::flag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    MissionControl,
    AppWindows,
    SpaceLeft,
    SpaceRight,
}

impl Hotkey {
    /// The symbolic hotkey id macOS stores it under.
    fn id(self) -> i32 {
        match self {
            Hotkey::MissionControl => 32,
            Hotkey::AppWindows => 33,
            Hotkey::SpaceLeft => 79,
            Hotkey::SpaceRight => 81,
        }
    }

    /// The macOS default: ⌃ and an arrow.
    fn default_chord(self) -> (u16, u64) {
        let k = match self {
            Hotkey::MissionControl => 126,
            Hotkey::AppWindows => 125,
            Hotkey::SpaceLeft => 123,
            Hotkey::SpaceRight => 124,
        };
        (k, flag::CTRL)
    }
}

/// Each hotkey's key and flags.
#[derive(Debug, Clone)]
pub struct Table([(u16, u64); 4]);

impl Table {
    pub fn defaults() -> Table {
        Table(ALL.map(Hotkey::default_chord))
    }

    /// This Mac's own shortcuts, where it has them on.
    pub fn read() -> Table {
        Table(ALL.map(|h| own(h.id()).unwrap_or_else(|| h.default_chord())))
    }

    pub fn get(&self, h: Hotkey) -> (u16, u64) {
        self.0[ALL.iter().position(|x| *x == h).unwrap_or(0)]
    }
}

const ALL: [Hotkey; 4] = [
    Hotkey::MissionControl,
    Hotkey::AppWindows,
    Hotkey::SpaceLeft,
    Hotkey::SpaceRight,
];

type GetValue = unsafe extern "C" fn(i32, *mut u16, *mut u16, *mut u32) -> i32;
type IsEnabled = unsafe extern "C" fn(i32) -> bool;

fn own(id: i32) -> Option<(u16, u64)> {
    let get = sym(c"CGSGetSymbolicHotKeyValue")?;
    let on = sym(c"CGSIsSymbolicHotKeyEnabled")?;
    // SAFETY: the symbols have these signatures on every macOS that exports them (Sill reads
    // them the same way); the out-pointers are valid locals.
    unsafe {
        let get: GetValue = std::mem::transmute(get);
        let on: IsEnabled = std::mem::transmute(on);
        if !on(id) {
            return None;
        }
        let (mut ch, mut key, mut mods) = (0u16, 0u16, 0u32);
        (get(id, &mut ch, &mut key, &mut mods) == 0 && key != 0xFFFF)
            .then_some((key, u64::from(mods)))
    }
}

fn sym(name: &std::ffi::CStr) -> Option<*mut libc::c_void> {
    // SAFETY: RTLD_DEFAULT lookup of a NUL-terminated name; CoreGraphics is linked by this crate.
    let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    (!p.is_null()).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_control_arrows() {
        let t = Table::defaults();
        assert_eq!(t.get(Hotkey::MissionControl), (126, flag::CTRL));
        assert_eq!(t.get(Hotkey::SpaceRight), (124, flag::CTRL));
    }

    #[test]
    fn this_mac_has_a_mission_control_key() {
        let (k, _) = Table::read().get(Hotkey::MissionControl);
        assert!(k > 0);
    }
}
