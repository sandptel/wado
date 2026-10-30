//! The Clipboard surface: one computer's clipboard history, the same component on its Computer
//! page (in full) and in its home widget (`compact`: the newest few, copy and send only). Shared
//! as copies happen by `js/clip_share.js`; the history is `server::clip`'s, kept fresh by the
//! daemon — the desktop's `cliphist`, or the daemon's own last 50.
//!
//! | file | job |
//! |---|---|
//! | [`source`] | the history, from whichever link carries it |
//! | [`filter`] | the search box and the kind chips |
//! | [`tile`] | one entry and its actions |
//! | [`words`] | how an entry is said: where and when it was copied |
//! | [`switches`] | this phone's two sharing switches for the computer |
//! | [`dock`] | the live region and one-tap chip, over everything |
//!
//! Keyboard and screen reader complete: every action is a labelled button of at least 44 px,
//! each entry is said whole once, and what sharing did is said in the dock's live region.

pub mod dock;
pub mod filter;
pub mod source;
pub mod switches;
pub mod tile;
pub mod words;

use dioxus::prelude::*;
use wado_protocol::ClipKind;

use crate::{bridge, state::Ui, ui::widgets::Icon};

/// How many entries the widget shows.
const COMPACT: usize = 3;

#[component]
pub fn Clipboard(
    ui: Ui,
    /// The computer, by key and by name.
    pk: String,
    name: String,
    /// It is the selected computer (its own link), else one a light link reaches.
    selected: bool,
    /// The widget's short form.
    #[props(default)]
    compact: bool,
) -> Element {
    let query = use_signal(String::new);
    let kind = use_signal(|| None::<ClipKind>);
    let h = source::of(ui, &pk, selected);
    if !h.loaded {
        return rsx! { div { class: "clipnote", span { class: "wspin" } "Asking {name}…" } };
    }
    let q = query().to_lowercase();
    let mut shown: Vec<_> = h
        .entries
        .iter()
        .filter(|e| compact || kind().is_none_or(|k| e.kind == k))
        .filter(|e| compact || q.is_empty() || e.preview.to_lowercase().contains(&q))
        .cloned()
        .collect();
    if compact {
        shown.truncate(COMPACT);
    }
    let count = h.entries.len();
    let pk_send = pk.clone();
    let more = count.saturating_sub(shown.len());

    rsx! {
        div { class: if compact { "clipsurf compact" } else { "clipsurf" },
            if !compact && h.available && count > 0 {
                div { class: "cliptools", {filter::render(query, kind, &h.entries)} }
            }
            if h.available {
                ul {
                    class: "cliplist",
                    "aria-label": "{name}'s clipboard, newest first",
                    // FLIP: entries glide to their new place when the list changes.
                    onmounted: move |_| bridge::call("window.__wado.clipAnimate();".to_string()),
                    for (i, e) in shown.iter().cloned().enumerate() {
                        tile::Tile {
                            key: "{e.id}",
                            img: h.imgs.get(&e.id).cloned(),
                            entry: e,
                            pk: pk.clone(),
                            computer: name.clone(),
                            selected,
                            i,
                            compact,
                        }
                    }
                }
                if shown.is_empty() {
                    p { class: "clipnote quiet",
                        Icon { name: "clip" }
                        if count == 0 { "Nothing copied on {name} yet." } else { "Nothing matches." }
                    }
                }
                if compact && more > 0 {
                    p { class: "clipmore", "{more} more on its page" }
                }
            } else {
                p { class: "clipnote", Icon { name: "info" } "{h.error}" }
            }
            button {
                class: "btn clipsend",
                "data-clip": "paste",
                "data-key": "{pk_send}",
                Icon { name: "paste" }
                "Send this phone's clipboard"
            }
        }
    }
}
