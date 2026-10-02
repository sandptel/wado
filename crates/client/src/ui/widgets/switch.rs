//! A setting that is on or off: title, one-line subtitle, the switch, and an optional ⓘ.

use dioxus::prelude::*;

use super::{Info, When, WhenBadge};

#[component]
pub fn SwitchRow(
    title: String,
    sub: String,
    on: bool,
    #[props(default)] disabled: bool,
    #[props(default)] when: Option<When>,
    #[props(default)] info: Option<String>,
    ontoggle: EventHandler<bool>,
) -> Element {
    rsx! {
        div { class: "switchrow",
            div { class: "swtext",
                b { "{title}" if let Some(w) = when { WhenBadge { when: w } } }
                small { "{sub}" }
            }
            if let Some(text) = info { Info { text } }
            button {
                class: if on { "sw on" } else { "sw" },
                role: "switch",
                "aria-checked": "{on}",
                "aria-label": "{title}",
                disabled,
                onclick: move |_| ontoggle.call(!on),
            }
        }
    }
}
