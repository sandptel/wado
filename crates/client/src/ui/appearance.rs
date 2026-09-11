//! Appearance group: the base16 scheme.
//!
//! Its own group rather than part of Live because "what colour is the UI" is a different kind
//! of answer from "how fast does scrolling feel" — and because the paste box is tall enough
//! to bury the controls next to it.

use dioxus::prelude::*;

use crate::{state::Ui, theme};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let custom = (s.theme_custom)();
    let pasted_ok = theme::parse(&custom).is_some();

    rsx! {
        label { "Theme" }
        select {
            value: "{(s.theme)()}",
            disabled: pasted_ok,
            onchange: move |e| {
                let v = e.value();
                s.theme.set(v.clone());
                theme::apply(&v, &(s.theme_custom)());
            },
            for name in theme::SCHEMES {
                option { key: "{name}", value: "{name}", "{name}" }
            }
        }

        details { class: "sub",
            summary { "Paste a base16 scheme" }
            p { class: "hint",
                "Any published base16 scheme — the YAML, or just its 16 hex values in order. \
                 A pasted scheme overrides the picker; clear the box to go back."
            }
            textarea {
                rows: "6",
                placeholder: "base00: \"1d2021\"\nbase01: \"3c3836\"\n…",
                value: "{custom}",
                oninput: move |e| {
                    let v = e.value();
                    s.theme_custom.set(v.clone());
                    theme::apply(&(s.theme)(), &v);
                },
            }
            // Silence is the wrong answer for a paste box: without this, a scheme that failed
            // to parse looks identical to one that was simply ignored.
            if !custom.trim().is_empty() {
                p {
                    class: if pasted_ok { "hint ok" } else { "hint bad" },
                    if pasted_ok { "Applied — 16 colours parsed." }
                    else { "Not applied — could not find 16 colours." }
                }
            }
        }
    }
}
