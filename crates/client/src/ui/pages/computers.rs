//! The computers and relays this device knows, in two sections: forgetting the computer whose
//! page is open ([`forget`], on its Connection & trust), and the relays a computer can be added
//! through ([`relays`], in the Add a computer flow).
//!
//! Forgetting is kept off the cards on purpose: it is rare and hard to undo, and a cross on
//! every card is a mis-tap away from losing one.

use dioxus::prelude::*;

use crate::{
    profile,
    state::{Ui, DEFAULT_RELAY},
    ui::widgets::Icon,
};

/// Every relay worth offering: the built-in one, the saved ones, and any a computer uses.
pub fn known_relays(ui: Ui) -> Vec<String> {
    let s = ui.set;
    let mut all = vec![DEFAULT_RELAY.to_string()];
    all.extend(s.relays.read().iter().cloned());
    all.extend(s.profiles.read().iter().map(|p| p.relay_url.clone()));
    let mut seen = Vec::new();
    all.retain(|r| {
        !r.is_empty() && !seen.contains(r) && {
            seen.push(r.clone());
            true
        }
    });
    all
}

/// Forget the selected computer — asked first. Its widgets go with it. Has its own scope.
pub fn forget(ui: Ui) -> Element {
    let mut s = ui.set;
    let mut ask = use_signal(|| false);
    let i = (s.profile)();
    let Some(p) = s.profiles.read().get(i).cloned() else {
        return rsx! {};
    };
    rsx! {
        div { class: "card",
            if ask() {
                div { class: "confirm",
                    b { "Forget {p.display()}?" }
                    p { class: "why", "It goes from this device's list, with its widgets. The computer itself is not touched, and you can add it again with its link or QR code." }
                    div { class: "actions",
                        button { class: "btn", onclick: move |_| ask.set(false), "Keep" }
                        button { class: "btn danger", onclick: move |_| {
                            let key = p.key();
                            s.profiles.write().remove(i);
                            crate::widget::forget_host(&mut s.widgets.write(), &key);
                            let n = s.profiles.read().len();
                            let cur = (s.profile)();
                            if cur >= n { s.profile.set(n.saturating_sub(1)); } else if cur > i { s.profile.set(cur - 1); }
                            ask.set(false);
                            if n > 0 { profile::select(ui, (s.profile)()); }
                            let mut view = ui.live.view;
                            view.set(crate::route::View::Home);
                        }, "Forget it" }
                    }
                }
            } else {
                button { class: "btn danger", onclick: move |_| ask.set(true), Icon { name: "x" } "Forget this computer" }
            }
        }
    }
}

/// The relays this device can reach computers through. Has its own scope: the one being typed.
pub fn relays(ui: Ui) -> Element {
    let mut s = ui.set;
    let relays = known_relays(ui);
    let mut new_relay = use_signal(String::new);
    rsx! {
        div { class: "card",
            div { class: "cardhead", "Relays" }
            p { class: "why", "The rendezvous servers this device can reach computers through. A computer lives on one; several can be saved." }
            for r in relays.iter().cloned() {
                {
                    let used = s.profiles.read().iter().any(|p| p.relay_url == r);
                    let builtin = r == DEFAULT_RELAY;
                    rsx! {
                        div { key: "{r}", class: "editrow",
                            span { class: "disc", style: "--hue:var(--base0C)", Icon { name: "net" } }
                            b { class: "mono", "{short(&r)}" }
                            if used { span { class: "whenbadge apply", "in use" } }
                            else if !builtin {
                                button { "aria-label": "Remove relay", onclick: move |_| s.relays.write().retain(|x| *x != r), Icon { name: "x" } }
                            }
                        }
                    }
                }
            }
            div { class: "row",
                input { r#type: "url", placeholder: "https://relay.example", value: "{new_relay}", oninput: move |e| new_relay.set(e.value()) }
            }
            button {
                class: "btn",
                disabled: !new_relay().starts_with("http"),
                onclick: move |_| {
                    let r = new_relay().trim().trim_end_matches('/').to_string();
                    if !s.relays.read().contains(&r) { s.relays.write().push(r); }
                    new_relay.set(String::new());
                },
                Icon { name: "plus" } "Add relay"
            }
        }
    }
}

pub fn short(relay: &str) -> &str {
    relay
        .trim_start_matches("https://")
        .trim_start_matches("wss://")
}
