//! The in-session gamepad's one switch, pushed to `js/gamepad.js`: drawn over the picture or not.
//! Everything else about it — its layout, what it drives, how it feels — is this device's
//! `gamepad { }` on the computer (`js/gpad/prefs.js`), changed on the Gamepad settings page
//! (`ui/computer/input/gamepad.rs`) and in the editor.
//!
//! The old `pad_mode` / `pad_scale` / `pad_opacity` / `pad_inset_*` settings are read once by the
//! layout migration (`js/gpad/prefs.js`) and otherwise left alone.

use crate::{bridge, state::Ui};

/// Push the switch to the bridge. Also called from [`crate::bridge::run`] after a reload, for the
/// same reason [`crate::ui::live::apply`] is — restoring the signals does not move the browser.
pub fn apply(ui: Ui) {
    bridge::call(format!(
        "window.__wado.setGamepad({{on:{}}});",
        (ui.set.pad_on)()
    ));
}
