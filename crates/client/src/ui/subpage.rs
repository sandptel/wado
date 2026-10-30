//! A settings page pushed over home or a computer's page: the page head — with an eyebrow naming
//! the computer, so a setting is never ambiguous about which computer it changes — the page,
//! and the Apply bar.
//!
//! It pushes in from the right on the spring (the page under it drifts 24% left and fades); Back
//! runs the reverse in `js/hero_flip.js` before the view changes, so the exit is seen too.

use dioxus::prelude::*;

use crate::{
    bridge,
    route::View,
    state::Ui,
    ui::{cc::apply_bar, pages, widgets::Icon},
};

pub fn render(ui: Ui) -> Element {
    let View::Sub(pk, page) = (ui.live.view)() else {
        return rsx! {};
    };
    let gone = (ui.live.session_on)();
    let (name, hue) = match pk.as_deref().and_then(|k| crate::route::index_of(ui, k)) {
        Some(i) => (ui.set.profiles.read()[i].display(), crate::profile::hue(i)),
        None => (crate::profile::current_name(ui), "0D"),
    };
    let eyebrow = if page.parent() == crate::ui::pages::Page::Input {
        format!("{name} · Virtual input")
    } else {
        name.clone()
    };
    let back_to = if page.parent() == crate::ui::pages::Page::Input {
        "Back to Virtual input".to_string()
    } else if pk.is_some() {
        format!("Back to {name}")
    } else {
        "Back to home".to_string()
    };
    rsx! {
        section {
            id: "sub",
            key: "{page:?}",
            class: if gone { "screen gone" } else { "screen" },
            style: "--hue:var(--base{hue})",
            "aria-label": "{page.title()}",
            div { class: "spage",
                div { class: "phead",
                    button {
                        class: "iconbtn",
                        "aria-label": "{back_to}",
                        onclick: move |_| bridge::call("window.__wado.subClose();".to_string()),
                        Icon { name: "left" }
                    }
                    div { class: "pt",
                        if pk.is_some() {
                            span { class: "eyebrow", title: "{eyebrow}", i {} "{eyebrow}" }
                        }
                        h3 { "{page.title()}" }
                    }
                    crate::ui::computer::input::PageOpen { ui, page }
                }
                {pages::render(ui, page)}
            }
            {apply_bar::render(ui)}
        }
    }
}
