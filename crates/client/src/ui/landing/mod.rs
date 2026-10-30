//! Home — the screen before a session: the computers, what is running, this phone's settings,
//! and the widgets. Its parts live in [`crate::ui::home`]; this frames them, top to bottom.
//!
//! A page of its own rather than the control centre opened over nothing, because before a
//! session the only decision is *where*, and it deserves the whole screen. It dissolves into
//! the stream once the session is up, and recedes under a computer's page or a settings page
//! pushed over it.

mod install_hint;
mod pair;

use dioxus::prelude::*;

use crate::{
    route::View,
    state::Ui,
    ui::home::{cards, sessions, settings, widgets},
};

pub fn render(ui: Ui) -> Element {
    let on = (ui.live.session_on)();
    let view = (ui.live.view)();
    // Covered by a computer's page (which runs its own motion over home) or by a pushed page.
    // Either way it is hidden once that motion has run (CSS), and inert at once: a page under
    // another is never painted through it, tapped or focused.
    let under = matches!(view, View::Sub(None, _));
    let covered = view != View::Home;
    let class = match (on, under, covered) {
        (true, ..) => "gone",
        (_, true, _) => "under",
        (_, _, true) => "covered",
        _ => "",
    };

    rsx! {
        section { id: "landing", class: "{class}", "aria-hidden": "{on || covered}", inert: crate::ui::widgets::inert(on || covered),
            div { class: "wordmark", "wado" span { "." } }
            p { class: "tag", "Your computers, at this screen's own resolution." }
            // iOS Safari only, once: how to install, since it never offers to.
            {crate::ui::scoped(ui, "install_hint", install_hint::render)}
            // The computer does not know this device yet: pair it before anything else.
            {crate::ui::scoped(ui, "pair", pair::render)}
            div { class: "sect", span { "Computers" } }
            {crate::ui::scoped(ui, "cards", cards::render)}
            {crate::ui::scoped(ui, "sessions", sessions::render)}
            div { class: "sect", span { "On this phone" } }
            {settings::render(ui)}
            {crate::ui::scoped(ui, "widgets", widgets::render)}
        }
    }
}
