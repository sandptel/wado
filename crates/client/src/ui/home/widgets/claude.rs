//! The Claude widget: the sessions on its computer, the one that asks with Allow / Deny on it.
//! Its cards come over the selected computer's own link, so a widget for another computer says
//! so and opens that computer instead.

use dioxus::prelude::*;

use crate::{claude::CARDS, route, ui::claude::card::Card, ui::widgets::Icon};

use super::shell::Ctx;

#[component]
pub fn Claude(cx: Ctx) -> Element {
    let name = cx.name.clone();
    if !cx.r.selected {
        let (ui, key) = (cx.ui, cx.w.host.clone());
        return rsx! {
            button { class: "wnote quiet", onclick: move |_| route::open_computer(ui, &key),
                Icon { name: "term" } span { "Claude on {name} shows while {name} is the computer you're on — tap to open it." }
            }
        };
    }
    let Some(st) = CARDS() else {
        return rsx! { div { class: "wnote quiet", Icon { name: "term" } "Asking {name}…" } };
    };
    if !st.hooks && st.sessions.is_empty() {
        return rsx! { div { class: "wnote quiet", Icon { name: "info" } span { "Run " span { class: "mono", "wado claude hooks install" } " on {name} to see Claude here." } } };
    }
    let mut live: Vec<_> = st
        .sessions
        .into_iter()
        .filter(|s| s.state != "ended")
        .collect();
    // The one asking first.
    live.sort_by_key(|s| s.ask.is_none());
    if live.is_empty() {
        return rsx! { div { class: "wnote quiet", Icon { name: "term" } "No Claude sessions on {name}." } };
    }
    rsx! {
        div { class: "cllist", for c in live.into_iter().take(3) { Card { key: "{c.id}", c, compact: true } } }
    }
}
