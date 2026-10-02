//! The on-screen gamepad's settings, pushed to `js/gamepad.js`. Its page is `pages::gamepad`.

use crate::{bridge, state::Ui};

/// Push the whole pad config to the bridge.
///
/// The whole config, never one field: `js/gamepad.js` keeps a single object and re-applies it
/// wholesale, so a partial update is the one way the panel and the overlay could disagree.
/// Also called from [`crate::bridge::run`] after a reload, for the same reason
/// [`crate::ui::live::apply`] is — restoring the signals does not move the browser.
pub fn apply(ui: Ui) {
    let s = ui.set;
    bridge::call(format!(
        "window.__wado.setPadEdit({});\
         window.__wado.setGamepad({{on:{},mode:{},scale:{},opacity:{},insetX:{},insetY:{}}});",
        (ui.live.pad_edit)(),
        (s.pad_on)(),
        bridge::js(&(s.pad_mode)()),
        (s.pad_scale)(),
        (s.pad_opacity)(),
        (s.pad_inset_x)(),
        (s.pad_inset_y)(),
    ));
}
