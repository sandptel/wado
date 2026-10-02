//! Keyboard shortcuts the compositor keeps for itself — config.kdl's `binds { }`.
//!
//! Read from the live config on each press that has a modifier held, so an edit applies on the
//! next keystroke. A press that matches is eaten, and so is its release: an app that never saw
//! the press must not see a release either.

use smithay::input::keyboard::{KeysymHandle, ModifiersState, xkb};
use wado_protocol::WindowAction;

/// The window action bound to this press, if any.
pub fn action_for(mods: &ModifiersState, sym: &KeysymHandle<'_>) -> Option<WindowAction> {
    if !(mods.logo || mods.ctrl || mods.alt) {
        return None;
    }
    let cfg = wado_config::live::current();
    if cfg.binds.keys.is_empty() {
        return None;
    }
    // The unshifted latin key, so `Mod+Shift+q` and `Mod+Q` read the same on any layout.
    let pressed = sym
        .raw_latin_sym_or_raw_current_sym()
        .unwrap_or_else(|| sym.modified_sym());
    let name = xkb::keysym_get_name(pressed);
    let binds = cfg.binds.parsed().ok()?;
    binds.into_iter().find_map(|(c, action)| {
        let hit = c.logo == mods.logo
            && c.ctrl == mods.ctrl
            && c.alt == mods.alt
            && c.shift == mods.shift
            && c.key.eq_ignore_ascii_case(&name);
        hit.then(|| to_action(&action)).flatten()
    })
}

fn to_action(name: &str) -> Option<WindowAction> {
    Some(match name {
        "close-window" => WindowAction::Close,
        "maximize" => WindowAction::Maximize,
        "minimize" => WindowAction::Minimize,
        "focus-next" => WindowAction::CycleFocus,
        "back" => WindowAction::Back,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_config_action_maps() {
        for a in wado_config::schema::binds::ACTIONS {
            assert!(super::to_action(a).is_some(), "{a}");
        }
    }
}
