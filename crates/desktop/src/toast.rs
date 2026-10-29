// SPDX-License-Identifier: AGPL-3.0-only
//! One line at the bottom: what an action did, or why it did not.

use slint::ComponentHandle;

use crate::{App, AppWindow};

pub fn say(ui: &AppWindow, text: &str, tone: &str) {
    let a = ui.global::<App>();
    a.set_toast(text.into());
    a.set_toast_tone(tone.into());
    a.set_toast_serial(a.get_toast_serial() + 1);
}

pub fn outcome(ui: &AppWindow, r: Result<String, String>) {
    match r {
        Ok(s) => say(ui, &s, "good"),
        Err(e) => say(ui, &e, "bad"),
    }
}
