//! The computer's clipboard, as a rail down the landing page's right edge: newest first, pinned
//! on top, searchable, filtered by kind, and a button that sends this device's clipboard to the
//! computer. On a narrow screen the rail is a drawer behind a tab on the edge.
//!
//! - [`filter`] — the search box and the kind chips.
//! - [`tile`] — one entry.
//!
//! The history is `server::clip`'s, kept fresh by the daemon; this only draws it.

mod filter;
mod tile;

use dioxus::prelude::*;
use wado_protocol::ClipKind;

use crate::{bridge, state::Ui, ui::widgets::Icon};

pub fn render(ui: Ui) -> Element {
    let hist = (ui.live.clip_hist)();
    let imgs = (ui.live.clip_imgs)();
    let query = use_signal(String::new);
    let kind = use_signal(|| None::<ClipKind>);
    let mut open = use_signal(|| false);
    if !hist.loaded {
        return rsx! {};
    }
    let q = query().to_lowercase();
    let shown: Vec<_> = hist
        .entries
        .iter()
        .filter(|e| kind().is_none_or(|k| e.kind == k))
        .filter(|e| q.is_empty() || e.preview.to_lowercase().contains(&q))
        .cloned()
        .collect();
    let count = hist.entries.len();

    rsx! {
        button {
            class: if open() { "cliptab on" } else { "cliptab" },
            "aria-label": "The computer's clipboard",
            onclick: move |_| open.toggle(),
            Icon { name: "clip" }
            if count > 0 { span { "{count}" } }
        }
        aside { id: "cliprail", class: if open() { "open" } else { "" },
            header { class: "clipbar",
                b { "Clipboard" }
                button {
                    class: "clippaste",
                    title: "Put this device's clipboard on the computer",
                    onclick: move |_| bridge::call("window.__wado.clipPaste();".to_string()),
                    Icon { name: "paste" }
                    "Send mine"
                }
            }
            if hist.available {
                {filter::render(query, kind, &hist.entries)}
                div {
                    class: "cliplist",
                    onmounted: move |_| bridge::call("window.__wado.clipAnimate();".to_string()),
                    for (i, e) in shown.iter().cloned().enumerate() {
                        tile::Tile { key: "{e.id}", img: imgs.get(&e.id).cloned(), entry: e, i }
                    }
                    if shown.is_empty() {
                        p { class: "clipnote",
                            if count == 0 { "Nothing copied on the computer yet." } else { "Nothing matches." }
                        }
                    }
                }
            } else {
                p { class: "clipnote", "{hist.error}" }
            }
        }
    }
}
