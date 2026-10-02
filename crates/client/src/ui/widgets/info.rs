//! ⓘ — the long explanation behind a control, folded away until asked for.
//!
//! The text is kept, not cut: these paragraphs record why a setting exists and what it costs,
//! and the people who need that are exactly the ones who will tap for it.

use dioxus::prelude::*;

use super::Icon;

#[component]
pub fn Info(text: String) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        button {
            class: if open() { "infobtn on" } else { "infobtn" },
            "aria-label": "More about this",
            "aria-expanded": "{open()}",
            onclick: move |_| open.set(!open()),
            Icon { name: "info" }
        }
        div { class: if open() { "infotext open" } else { "infotext" },
            p { "{text}" }
        }
    }
}
