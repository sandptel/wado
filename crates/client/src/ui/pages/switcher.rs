//! Window switcher: where the dial sits and how much of it shows.

use dioxus::prelude::*;

use super::opts;
use crate::{
    state::Ui,
    ui::{live::apply, widgets::Seg},
};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    rsx! {
        div { class: "card",
            div { class: "cardhead", "Direction" }
            Seg {
                options: opts(&[("vertical", "Vertical"), ("horizontal", "Horizontal")]),
                value: (s.dial_orient)(),
                onpick: move |v| { s.dial_orient.set(v); apply(ui); },
            }
            div { class: "cardhead", "Apps visible" }
            Seg {
                options: opts(&[("3", "3"), ("5", "5")]),
                value: (s.dial_count)(),
                onpick: move |v| { s.dial_count.set(v); apply(ui); },
            }
        }
        div { class: "card",
            div { class: "cardhead", "Position" }
            div { class: "anchors", role: "radiogroup",
                for (v, label) in [
                    ("top-left", "↖"), ("top", "↑"), ("top-right", "↗"),
                    ("left", "←"), ("", ""), ("right", "→"),
                    ("bottom-left", "↙"), ("bottom", "↓"), ("bottom-right", "↘"),
                ] {
                    if v.is_empty() {
                        span { key: "mid{v}", class: "anchor-mid" }
                    } else {
                        button {
                            key: "{v}",
                            class: if (s.dial_pos)() == v { "anchor on" } else { "anchor" },
                            "aria-label": "{v}",
                            onclick: move |_| { s.dial_pos.set(v.to_string()); apply(ui); },
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
