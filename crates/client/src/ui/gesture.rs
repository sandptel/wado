//! What a three-finger swipe does, once `js/gestures.js` has recognised it.

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::console};

pub fn run(ui: Ui, action: &str) {
    let mut live = ui.live;
    if !(live.session_on)() && !matches!(action, "control-centre") {
        return;
    }
    let window = |a: &str| bridge::call(format!("window.__wado.windowAction({});", bridge::js(&a)));
    match action {
        "app-drawer" => {
            let open = !(live.drawer_open)();
            live.drawer_open.set(open);
            if open {
                bridge::call("window.__wado.requestApps();".to_string());
            }
        }
        "control-centre" => live.cc_open.set(!(live.cc_open)()),
        "keyboard" => bridge::call("window.__wado.oskToggle();".to_string()),
        "back" => bridge::call("window.__wado.back();".to_string()),
        "focus-next" => window("cycle_focus"),
        "close-window" => window("close"),
        "maximize" => window("maximize"),
        "minimize" => window("minimize"),
        "shells" => console::open_shells(ui),
        _ => {}
    }
}
