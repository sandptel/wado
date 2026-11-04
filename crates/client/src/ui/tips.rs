//! Just-in-time hints, on the web (the app's `hints/tips.dart`): each shown once, the first time it
//! is relevant, then never again unless brought back; and the first-run tour — the shared cards
//! (`wado_ux::tour`), worded for the form in hand, after the first computer is paired.

use dioxus::prelude::*;

use crate::{state::Ui, ui::widgets::Icon};

pub fn seen(ui: Ui, id: &str) -> bool {
    ui.set.tips_seen.read().iter().any(|t| t == id)
}

pub fn mark(ui: Ui, id: &str) {
    if !seen(ui, id) {
        let mut t = ui.set.tips_seen;
        t.write().push(id.to_string());
    }
}

/// A once-only bubble while `id` is unseen: gone on "Got it" or after 10 s.
#[component]
pub fn Tip(ui: Ui, id: String, title: String, words: String) -> Element {
    let shown = !seen(ui, &id);
    let i2 = id.clone();
    use_future(move || {
        let id = i2.clone();
        async move {
            crate::bridge::sleep(10_000).await;
            mark(ui, &id);
        }
    });
    if !shown {
        return rsx! {};
    }
    rsx! {
        div { class: "tip", role: "status",
            b { "{title}" }
            span { "{words}" }
            button { class: "tipok", onclick: move |_| mark(ui, &id), "Got it" }
        }
    }
}

/// The four cards, once, after the first pairing.
pub fn tour(ui: Ui) -> Element {
    let mut i = use_signal(|| 0usize);
    if seen(ui, "tour") || ui.set.profiles.read().is_empty() || (ui.live.session_on)() {
        return rsx! {};
    }
    let cards = wado_ux::tour::cards(&crate::profile::current_name(ui), crate::ui::sheet::form(ui));
    let n = cards.len();
    let c = cards[i().min(n - 1)].clone();
    let last = i() + 1 >= n;
    rsx! {
        div { class: "tour", role: "dialog", "aria-modal": "true", "aria-label": "Welcome to wado",
            div { class: "tourcard", key: "{i()}",
                div { class: "tourart", style: "--h: var(--base{c.hue:02X})",
                    match c.art.as_str() {
                        "shelf" => rsx! { span { class: "tourshelf", for (h, ic) in [("0D", "pad"), ("0B", "apps"), ("09", "play")] { span { key: "{h}", style: "--h: var(--base{h})", Icon { name: ic } } } } },
                        "?" => rsx! { span { class: "tourbig", "?" } },
                        a => rsx! { span { class: "tourbig", Icon { name: a.to_string() } } },
                    }
                }
                div { class: "tourdots", for k in 0..n { i { key: "{k}", class: if k == i() { "on" } else { "" } } } }
                b { "{c.title}" }
                p { "{c.words}" }
                div { class: "grow2",
                    button { class: "btn", onclick: move |_| mark(ui, "tour"), "Skip" }
                    button { class: "btn primary", onclick: move |_| if last { mark(ui, "tour") } else { i += 1 }, if last { "Start" } else { "Next" } }
                }
            }
        }
    }
}
