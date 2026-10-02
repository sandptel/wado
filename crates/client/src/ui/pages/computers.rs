//! Computers & relays — the deeper place for what the landing only picks from: rename a
//! computer, move it to another relay, forget it; and the relays this device knows.
//!
//! Kept off the landing on purpose: forgetting a computer or a relay is rare and hard to undo,
//! and a cross on every card is a mis-tap away from losing one.

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

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let list = s.profiles.read().clone();
    let relays = known_relays(ui);
    let mut forget = use_signal(|| None::<usize>);
    let mut new_relay = use_signal(String::new);

    rsx! {
        div { class: "sect", span { "Computers" } }
        if list.is_empty() {
            div { class: "card", p { class: "why", "No saved computers yet. Connect to one and it is saved here." } }
        }
        for (i, p) in list.into_iter().enumerate() {
            {
            let hint = if p.host.is_empty() { "Named after the computer once it connects".to_string() } else { p.host.clone() };
            rsx! {
            div { key: "{i}", class: "card",
                div { class: "streamhead",
                    span { class: "disc", Icon { name: "server" } }
                    div { class: "navtext", b { "{p.display()}" } small { "{p.address()}" } }
                }
                label { class: "field",
                    span { "Name" }
                    input {
                        r#type: "text", value: "{p.name}",
                        placeholder: "{hint}",
                        oninput: move |e| if let Some(x) = s.profiles.write().get_mut(i) { x.name = e.value(); },
                    }
                }
                label { class: "field",
                    span { "Relay" }
                    select {
                        value: "{p.relay_url}",
                        onchange: move |e| {
                            if let Some(x) = s.profiles.write().get_mut(i) { x.relay_url = e.value(); }
                            if (s.profile)() == i { profile::select(ui, i); }
                        },
                        for r in relays.iter().cloned() { option { key: "{r}", value: "{r}", "{short(&r)}" } }
                    }
                }
                label { class: "field",
                    span { "Remote ID" }
                    input {
                        r#type: "text", inputmode: "numeric", value: "{p.remote_id}",
                        oninput: move |e| if let Some(x) = s.profiles.write().get_mut(i) { x.remote_id = e.value(); },
                        onchange: move |_| if (s.profile)() == i { profile::select(ui, i); },
                    }
                }
                if forget() == Some(i) {
                    div { class: "confirm",
                        b { "Forget {p.display()}?" }
                        p { class: "why", "It goes from this device's list. The computer itself is not touched, and you can add it again with its link or QR code." }
                        div { class: "actions",
                            button { class: "btn", onclick: move |_| forget.set(None), "Keep" }
                            button { class: "btn danger", onclick: move |_| {
                                s.profiles.write().remove(i);
                                let n = s.profiles.read().len();
                                let cur = (s.profile)();
                                if cur >= n { s.profile.set(n.saturating_sub(1)); } else if cur > i { s.profile.set(cur - 1); }
                                forget.set(None);
                                if n > 0 { profile::select(ui, (s.profile)()); }
                            }, "Forget it" }
                        }
                    }
                } else {
                    button { class: "btn danger", onclick: move |_| forget.set(Some(i)), Icon { name: "x" } "Forget this computer" }
                }
            }
            }
            }
        }

        div { class: "sect", span { "Relays" } }
        div { class: "card",
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

fn short(relay: &str) -> &str {
    relay
        .trim_start_matches("https://")
        .trim_start_matches("wss://")
}
