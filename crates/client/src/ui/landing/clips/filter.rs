//! The rail's search box and kind chips, each chip with its count.

use dioxus::prelude::*;
use wado_protocol::{ClipEntry, ClipKind};

use crate::ui::widgets::Icon;

const CHIPS: [(Option<ClipKind>, &str, &str); 4] = [
    (None, "clip", "All"),
    (Some(ClipKind::Text), "text", "Text"),
    (Some(ClipKind::Link), "link", "Links"),
    (Some(ClipKind::Image), "image", "Images"),
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
                placeholder: "Search",
                value: "{query}",
                oninput: move |e| query.set(e.value()),
            }
        }
        div { class: "clipchips", role: "tablist",
            for (k, icon, label) in CHIPS {
                button {
                    key: "{label}",
                    class: if kind() == k { "clipchip on" } else { "clipchip" },
                    title: "{label}",
                    "aria-label": "{label}",
                    onclick: move |_| kind.set(k),
                    Icon { name: icon }
                    span { {entries.iter().filter(|e| k.is_none_or(|k| e.kind == k)).count().to_string()} }
                }
            }
        }
    }
}
