//! The input faces: Touchpad, Keys, Quick actions, Gamepad (spec §3). The active one is accent
//! and a full pill (the Morphing Toggle Rule). A desktop has no Touchpad face: a real mouse makes
//! it pointless (§4).
//!
//! In the cockpit the quick rail shows only the active face; tapping it brings the console back,
//! where all four can be reached (§6 S9). That button is [`rail_button`], on the rail itself.
//!
//! A change goes to `js/console_prefs.js` (this device's `console { face }`), which tells the page
//! to animate the relayout and tells us back through `consolePrefs`.

use dioxus::prelude::*;

use super::layout::Face;
use crate::{bridge, state::Ui, ui::widgets::Icon};

fn choose(face: Face) {
    bridge::call(format!(
        "window.__wado.consolePrefs.set('face', {});",
        bridge::js(&face.id())
    ));
}

pub fn switcher(ui: Ui, active: Face) -> Element {
    let touch = (ui.live.stage)().form.touch();
    rsx! {
        div { class: "cfaces", role: "toolbar", "aria-label": "Input",
            for f in Face::ALL.into_iter().filter(|f| touch || *f != Face::Pad) {
                button {
                    key: "{f.id()}",
                    class: if f == active { "barbtn fbtn on" } else { "barbtn fbtn" },
                    "data-face": "{f.id()}",
                    title: "{f.label()}",
                    "aria-label": "{f.label()}",
                    "aria-pressed": "{f == active}",
                    onclick: move |_| choose(f),
                    Icon { name: f.icon() }
                }
            }
        }
    }
}

/// The cockpit rail's face button: the active face's icon, and the way back to the console.
pub fn rail_button(ui: Ui) -> Element {
    let face = (ui.live.stage)().face();
    rsx! {
        button {
            class: "barbtn railface",
            "data-only": "cockpit",
            "data-arrange": "console",
            title: "{face.label()} — back to the console",
            "aria-label": "{face.label()}: back to the console to change it",
            Icon { name: face.icon() }
        }
    }
}
