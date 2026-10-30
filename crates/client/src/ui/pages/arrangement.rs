//! The Pocket Console's two preferences (spec §8): how a session opens, and the deck's input
//! face. This device's own (`device { console { … } }` on the computer, mirrored in the browser by
//! `js/console_prefs.js`), so they are written there rather than into the settings blob.

use dioxus::prelude::*;

use super::opts;
use crate::{bridge, state::Ui, ui::widgets::Seg};

const OPENS: &[(&str, &str)] = &[("console", "Console"), ("full", "Full screen")];
const FACES: &[(&str, &str)] = &[
    ("auto", "Auto"),
    ("pad", "Touchpad"),
    ("keys", "Keys"),
    ("quick", "Quick"),
    ("gamepad", "Gamepad"),
];

fn set(field: &'static str, v: String) {
    bridge::call(format!(
        "window.__wado.consolePrefs.set({}, {});",
        bridge::js(&field),
        bridge::js(&v)
    ));
}

pub fn render(ui: Ui) -> Element {
    let st = (ui.live.stage)();
    let open = if st.open_pref.is_empty() {
        "console".to_string()
    } else {
        st.open_pref.clone()
    };
    let face = if st.face_pref.is_empty() {
        "auto".to_string()
    } else {
        st.face_pref.clone()
    };
    rsx! {
        div { class: "card",
            div { class: "cardhead", "Open sessions in" }
            Seg { options: opts(OPENS), value: open.clone(), onpick: move |v| set("open", v) }
            p { class: "hint",
                if open == "full" {
                    "The picture fills the screen and the controls float over it."
                } else {
                    "The picture sits inset, with the controls around it. Full screen is one tap away."
                }
            }
            div { class: "cardhead", "Input face" }
            Seg { options: opts(FACES), value: face.clone(), onpick: move |v| set("face", v) }
            p { class: "hint",
                match face.as_str() {
                    "pad" => "A touchpad under the picture moves the computer's pointer.",
                    "keys" => "Super, Ctrl, Alt, Shift, Esc, Tab and the arrows under the picture.",
                    "quick" => "Workspaces and playback under the picture.",
                    "gamepad" => "The on-screen gamepad under the picture.",
                    _ => "The touchpad on a touchscreen, quick actions with a mouse.",
                }
            }
        }
    }
}
