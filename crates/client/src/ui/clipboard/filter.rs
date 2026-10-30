//! The search box and the kind chips, each chip with its count.

use dioxus::prelude::*;
use wado_protocol::{ClipEntry, ClipKind};

use crate::ui::widgets::Icon;

const CHIPS: [(Option<ClipKind>, &str, &str); 4] = [
    (None, "clip", "All"),
    (Some(ClipKind::Text), "text", "Text"),
    (Some(ClipKind::Link), "link", "Links"),
    (Some(ClipKind::Image), "image", "Pictures"),
];

pub fn render(
    mut query: Signal<String>,
    mut kind: Signal<Option<ClipKind>>,
    entries: &[ClipEntry],
) -> Element {
    rsx! {
        label { class: "clipsearch",
            Icon { name: "search" }
            input {
                r#type: "search",
                placeholder: "Search the clipboard",
                "aria-label": "Search the clipboard",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
            }
        }
        div { class: "clipkinds", role: "group", "aria-label": "Show",
            for (k, icon, label) in CHIPS {
                {
                    let n = entries.iter().filter(|e| k.is_none_or(|k| e.kind == k)).count();
                    rsx! {
                        button {
                            key: "{label}",
                            class: if kind() == k { "clipkind on" } else { "clipkind" },
                            "aria-pressed": "{kind() == k}",
                            "aria-label": "{label}, {n}",
                            onclick: move |_| kind.set(k),
                            Icon { name: icon }
                            span { class: "ck", "{label}" }
                            span { class: "cn", "{n}" }
                        }
                    }
                }
            }
        }
    }
}
