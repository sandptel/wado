//! Appearance: the base16 scheme, and the shape and motion of the shell around it.

use dioxus::prelude::*;

use super::opts;
use crate::{state::Ui, theme, ui::widgets::Seg};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let custom = (s.theme_custom)();
    let pasted_ok = theme::parse(&custom).is_some();
    let current = (s.theme)();
    let look = move || theme::apply_look(&(s.radius)(), &(s.motion)(), &(s.accent)());

    rsx! {
        div { class: "themes",
            for (name, c) in theme::schemes().into_iter() {
                button {
                    key: "{name}",
                    class: if name == current && !pasted_ok { "theme on" } else { "theme" },
                    style: "background:#{c[0]};color:#{c[5]}",
                    onclick: move |_| {
                        s.theme.set(name.clone());
                        s.theme_custom.set(String::new());
                        theme::apply(&name, "");
                    },
                    span { class: "pv", style: "background:#{c[1]}",
                        i { style: "left:8px;top:8px;width:40%;height:7px;background:#{c[5]}" }
                        i { style: "left:8px;top:21px;width:60%;height:5px;background:#{c[3]}" }
                        i { style: "right:8px;bottom:8px;width:24px;height:14px;background:#{c[13]}" }
                    }
                    span { class: "strip",
                        for (k, h) in c[8..].iter().enumerate() { span { key: "{k}", style: "background:#{h}" } }
                    }
                    b { "{name}" }
                }
            }
        }

        div { class: "card",
            div { class: "cardhead", "Accent" }
            div { class: "accents", role: "radiogroup",
                for slot in ["08", "09", "0A", "0B", "0C", "0D", "0E", "0F"] {
                    button {
                        key: "{slot}",
                        class: if (s.accent)() == slot { "accent on" } else { "accent" },
                        style: "--hue:var(--base{slot})",
                        "aria-label": "base{slot}",
                        onclick: move |_| { s.accent.set(slot.to_string()); look(); },
                    }
                }
            }
            div { class: "cardhead", "Corners" }
            Seg {
                options: opts(&[("sharp", "Sharp"), ("round", "Round"), ("pill", "Pill")]),
                value: (s.radius)(),
                onpick: move |v| { s.radius.set(v); look(); },
            }
            div { class: "cardhead", "Motion" }
            Seg {
                options: opts(&[("full", "Full"), ("reduced", "Reduced"), ("off", "Off")]),
                value: (s.motion)(),
                onpick: move |v| { s.motion.set(v); look(); },
            }
        }

        div { class: "card",
            div { class: "cardhead", "Paste a base16 scheme" }
            p { class: "why", "Any published scheme — the YAML, or its 16 hex values in order. A pasted scheme wins; clear the box to go back." }
            textarea {
                rows: "5",
                placeholder: "base00: \"1d2021\"\nbase01: \"3c3836\"\n…",
                value: "{custom}",
                oninput: move |e| {
                    let v = e.value();
                    s.theme_custom.set(v.clone());
                    theme::apply(&(s.theme)(), &v);
                },
            }
            if !custom.trim().is_empty() {
                p { class: if pasted_ok { "why ok" } else { "why bad" },
                    if pasted_ok { "Applied — 16 colours parsed." } else { "Not applied — could not find 16 colours." }
                }
            }
        }
    }
}
