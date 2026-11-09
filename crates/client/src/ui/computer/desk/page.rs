//! Apps: every app on the Mac — a search box, what is open now, then A–Z. Pushed from the dock's
//! "All apps"; the dock under it keeps the list fresh.

use dioxus::prelude::*;

use super::tile::{Menu, Tile};
use crate::state::Ui;

pub fn render(ui: Ui) -> Element {
    let mut query = use_signal(String::new);
    let q = query();
    let apps = ui.live.apps.read();
    let open = super::open_now(&apps, &q);
    let all = super::a_to_z(&apps, &q);
    // The keyboard comes up only where it does not cover half the page.
    let wide = crate::ui::sheet::form(ui) != wado_ux::form::Form::Phone;

    rsx! {
        input {
            class: "dsearch",
            r#type: "text",
            inputmode: "search",
            enterkeyhint: "search",
            autocomplete: "off",
            placeholder: "Search apps",
            "aria-label": "Search apps",
            value: "{q}",
            oninput: move |e| query.set(e.value()),
            onmounted: move |e| if wide { spawn(async move { let _ = e.data().set_focus(true).await; }); },
        }
        if !open.is_empty() {
            div { class: "sect", span { "Open now" } }
            div { class: "dgrid",
                for a in open.iter() {
                    Tile { key: "{a.exec}", app: (*a).clone(), named: true }
                }
            }
        }
        div { class: "sect", span { "All apps" } }
        if all.is_empty() {
            p { class: "hint dnone",
                if apps.is_empty() { "Asking for its apps…" } else { "No app is called “{q.trim()}”." }
            }
        } else {
            div { class: "dgrid",
                for a in all.iter() {
                    Tile { key: "{a.exec}", app: (*a).clone(), named: true }
                }
            }
        }
        Menu { ui }
    }
}
