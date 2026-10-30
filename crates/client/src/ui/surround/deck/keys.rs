//! The Keys face: the keys a browser swallows or a phone keyboard lacks — Super, Ctrl, Alt, Shift
//! (latching: tap for the next key, tap twice to hold), Esc, Tab, the arrows — and Type, which
//! raises the keyboard (`data-osk`, caught in `js/chrome.js` inside the tap). The look is the
//! trackpad rail's (`.tpkey`, `.tptype`); `js/deck_keys.js` sends them to the session.

use dioxus::prelude::*;

use crate::ui::widgets::Icon;

const MODS: &[(&str, &str)] = &[
    ("super", "Super"),
    ("ctrl", "Ctrl"),
    ("alt", "Alt"),
    ("shift", "Shift"),
];
const KEYS: &[(&str, &str, &str)] = &[
    ("esc", "Esc", "Escape"),
    ("tab", "Tab", "Tab"),
    ("left", "←", "Left arrow"),
    ("right", "→", "Right arrow"),
    ("up", "↑", "Up arrow"),
    ("down", "↓", "Down arrow"),
];

pub fn render() -> Element {
    rsx! {
        div { class: "dkeys",
            for (id, label) in MODS.iter().copied() {
                button { key: "{id}", class: "tpkey", "data-dmod": "{id}", "aria-pressed": "false", "{label}" }
            }
            for (id, label, name) in KEYS.iter().copied() {
                button { key: "{id}", class: "tpkey", "data-dkey": "{id}", "aria-label": "{name}", "{label}" }
            }
            button { class: "tptype", "data-osk": "1", "aria-label": "Type on the computer",
                Icon { name: "kbd" }
                span { "Type" }
            }
        }
    }
}
