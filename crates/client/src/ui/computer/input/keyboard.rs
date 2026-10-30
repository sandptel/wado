//! Keyboard settings (§5.3): a preview that types a line and then acts out a shortcut (Super
//! latches, "1" goes, "Super + 1 → workspace 1"), the keys a phone keyboard lacks — chosen here,
//! shown in the order added, above the keyboard on the touchpad screen and the console's Keys
//! face — and how shortcuts work.

use dioxus::prelude::*;

use super::prefs::{self, KEYS};
use crate::{bridge, state::Ui, ui::widgets::Icon};

/// Every key that can sit above the keyboard: `(name as the computer knows it, label)`. The
/// function row is one chip.
pub const CHOICES: &[(&str, &str)] = &[
    ("esc", "Esc"),
    ("tab", "Tab"),
    ("left", "←"),
    ("up", "↑"),
    ("down", "↓"),
    ("right", "→"),
    ("home", "Home"),
    ("end", "End"),
    ("pageup", "PgUp"),
    ("pagedown", "PgDn"),
    ("delete", "Del"),
    ("enter", "Enter"),
    ("f1", "F1–F12"),
];

pub fn render(ui: Ui) -> Element {
    let name = crate::profile::current_name(ui);
    let keys = KEYS.read().clone();
    let mut more = use_signal(|| false);
    rsx! {
        div { class: "vipage",
            div { class: "vipreview", "aria-hidden": "true",
                div { class: "vicap", span { "Preview" } span { class: "videmo", "data-vi-demo": "keyboard", "typing" } }
                div { class: "vikpv", "data-vi": "keyboard" }
            }
            div { class: "sect", span { "Keys above the keyboard" } }
            div { class: "card vicard",
                p { class: "viinfo", Icon { name: "info" }
                    span { "The keys a phone keyboard does not have. Tap to add or take away; they appear in the order you add them." }
                }
                div { class: "vikeypick", role: "group", "aria-label": "Keys above the keyboard",
                    for (k, label) in CHOICES.iter().copied() {
                        button {
                            key: "{k}",
                            class: "vikey mono",
                            "aria-pressed": "{keys.iter().any(|x| x == k)}",
                            onclick: move |_| {
                                bridge::call(format!("window.__wado.kbPrefs.toggle({});", bridge::js(&k)));
                                prefs::demo("keyboard", "keys");
                            },
                            "{label}"
                        }
                    }
                }
            }
            div { class: "sect", span { "Shortcuts" } }
            div { class: "card vicard",
                div { class: "vimodexp", "aria-hidden": "true",
                    div { span { class: "mono", "Super" } "Off" }
                    div { span { class: "mono latched", "Super" } "Tap: the next key" }
                    div { span { class: "mono locked", "Super" } "Tap twice: held" }
                }
                button { class: "vimore", "aria-expanded": "{more()}", onclick: move |_| more.toggle(),
                    if more() { "Less" } else { "Why the next key is a shortcut" }
                }
                if more() {
                    p { class: "videtail",
                        "While Super, Ctrl or Alt is waiting, the next letter you type is sent as a key, not as text, so “Super, then 1” switches {name}'s workspace. Dictation and whole words are always text."
                    }
                }
            }
            div { class: "sect", span { "Typing" } }
            div { class: "card vicard",
                p { class: "viinfo", Icon { name: "info" }
                    span { "Autocorrect, swipe typing and voice dictation all work: wado sends what your keyboard commits, in any language." }
                }
            }
        }
    }
}
