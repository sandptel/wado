//! This app: which wado this phone runs, whether it was signed, an update waiting for the next
//! launch, and an update that was refused — what the service worker says ([`crate::app_info`]).
//! The head of Diagnostics: plain words first, the release key in mono under them.

use dioxus::prelude::*;

use crate::{state::Ui, ui::widgets::Icon};

pub fn render(ui: Ui) -> Element {
    let app = (ui.live.app)();
    let line = app.line();
    let sub = app.sub();
    let refused = app.refused_words();

    rsx! {
        div { class: "card appver", role: "group", "aria-label": "This app",
            div { class: "avhead",
                span { class: if app.pinned() && app.signed { "avdot ok" } else { "avdot" } }
                b { class: "avline", "{line}" }
            }
            p { class: "avsub", "{sub}" }
            if !app.key.is_empty() {
                p { class: "avkey", "release key {app.key}" }
            }
            if !app.pending.is_empty() {
                p { class: "avnote", role: "status",
                    Icon { name: "update" }
                    span { "Update {app.pending} is ready — reopen wado to use it." }
                }
            }
            if let Some(words) = refused {
                div { class: "avnote warn", role: "status",
                    Icon { name: "alert" }
                    span {
                        b { "{words}" }
                        small {
                            if app.refused_version.is_empty() { "{app.refused}" } else { "{app.refused_version}: {app.refused}" }
                            ". wado keeps using the version above."
                        }
                    }
                }
            }
        }
    }
}
