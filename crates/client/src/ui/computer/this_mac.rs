//! On this Mac: what the computer cannot do, each with its reason in plain words (a permission to
//! grant is said exactly as the daemon says it), and what is coming to macOS — the one place a
//! hidden control is accounted for (`crate::host_caps`). Only for a Mac.

use dioxus::prelude::*;

use crate::{host_caps, reach::Reach};

#[component]
pub fn ThisMac(r: Reach) -> Element {
    let Some(c) = r.config.as_ref().filter(|c| host_caps::mac(c)) else {
        return rsx! {};
    };
    let (later, off): (Vec<_>, Vec<_>) = c.host_caps.off.iter().cloned().partition(|o| o.later);
    if later.is_empty() && off.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "sx o5", style: "--n:5", "aria-label": "On this Mac",
            div { class: "sect", span { "On this Mac" } }
            div { class: "card",
                if !off.is_empty() {
                    div { class: "cardhead", "Not available" }
                    for o in off {
                        div { key: "{o.cap}", class: "navtext",
                            b { "{host_caps::label(&o.cap)}" }
                            small { "{o.why}" }
                        }
                    }
                }
                if !later.is_empty() {
                    div { class: "cardhead", span { class: "whenbadge later", "{host_caps::SOON}" } }
                    for o in later {
                        div { key: "{o.cap}", class: "navtext",
                            b { "{host_caps::label(&o.cap)}" }
                            small { "{o.why}" }
                        }
                    }
                }
            }
        }
    }
}
