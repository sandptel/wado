//! Trackpad: how this phone's trackpad for this computer is laid out — which way round, what
//! the rail beside the pad holds, its quick actions, its phone gestures, and whether the
//! keyboard opens with it.
//!
//! Saved on the computer, under this phone's own `device { trackpad }` block, which a device may
//! change without the settings grant (`server::config::link`). The trackpad itself reads the same
//! block, so a change here is what it opens with next.

use dioxus::prelude::*;
use wado_protocol::{pad::ActionInfo, TrackpadPrefs};

use crate::{
    bridge,
    state::Ui,
    ui::{
        pages::opts,
        widgets::{Icon, Seg, SwitchRow},
    },
};

/// What a phone with no block gets — `wado_config::schema::DeviceTrackpad::default`.
fn defaults() -> TrackpadPrefs {
    TrackpadPrefs {
        landscape: true,
        rail: "keys".into(),
        quick: [
            "workspace-prev",
            "workspace-next",
            "fullscreen",
            "play-pause",
        ]
        .map(String::from)
        .to_vec(),
        ..Default::default()
    }
}

/// Set `device.<this phone>.trackpad.<field>` on the computer.
fn set(field: &str, value: String) {
    bridge::call(format!(
        "window.__wado.configSetOwn({}, {});",
        bridge::js(&format!("trackpad.{field}")),
        bridge::js(&value)
    ));
}

const GESTURES: &[(&str, &str)] = &[
    ("edge-left", "Swipe in from the left edge"),
    ("edge-right", "Swipe in from the right edge"),
    ("four-tap", "Four-finger tap"),
    ("two-hold", "Two-finger hold"),
];

pub fn render(ui: Ui) -> Element {
    let t = (ui.live.host)().trackpad.unwrap_or_else(defaults);
    let actions = (ui.live.actions)();
    // Ask for the action list once this page is up.
    use_effect(move || bridge::call("window.__wado.actionsGet();".to_string()));
    let name_of = |id: &str, all: &[ActionInfo]| {
        all.iter()
            .find(|a| a.id == id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let quick = t.quick.clone();
    let spare: Vec<ActionInfo> = actions
        .iter()
        .filter(|a| !quick.contains(&a.id))
        .cloned()
        .collect();
    let n = quick.len();
    let put = move |list: Vec<String>| set("quick", crate::bridge::js(&list));

    rsx! {
        div { class: "card",
            div { class: "cardhead", "Held" }
            Seg {
                options: opts(&[("landscape", "Sideways"), ("upright", "Upright")]),
                value: if t.landscape { "landscape" } else { "upright" },
                onpick: move |v: String| set("landscape", (v == "landscape").to_string()),
            }
            SwitchRow {
                title: "Fullscreen",
                sub: "Lock the screen's orientation instead of drawing the pad turned",
                on: t.fullscreen,
                ontoggle: move |v: bool| set("fullscreen", v.to_string()),
            }
            SwitchRow {
                title: "Flip",
                sub: "Turned the other way round",
                on: t.flip,
                ontoggle: move |v: bool| set("flip", v.to_string()),
            }
            SwitchRow {
                title: "Keyboard on open",
                sub: "The keyboard is up when the trackpad opens",
                on: t.keyboard,
                ontoggle: move |v: bool| set("keyboard", v.to_string()),
            }
        }
        div { class: "card",
            div { class: "cardhead", "Beside the pad" }
            Seg {
                options: opts(&[("keys", "Keys"), ("actions", "Quick actions"), ("none", "Nothing")]),
                value: t.rail.clone(),
                onpick: move |v: String| set("rail", v),
            }
        }
        div { class: "card",
            div { class: "cardhead", "Quick actions" }
            if quick.is_empty() { p { class: "why", "None yet — add some below." } }
            for (i, id) in quick.iter().cloned().enumerate() {
                {
                    let (up, down, gone) = (quick.clone(), quick.clone(), quick.clone());
                    let (i1, i2, id2) = (i, i, id.clone());
                    rsx! {
                        div { key: "{id}", class: "editrow",
                            b { "{name_of(&id, &actions)}" }
                            button { "aria-label": "Move up", disabled: i == 0,
                                onclick: move |_| { let mut l = up.clone(); l.swap(i1, i1 - 1); put(l); },
                                Icon { name: "up" } }
                            button { "aria-label": "Move down", disabled: i + 1 == n,
                                onclick: move |_| { let mut l = down.clone(); l.swap(i2, i2 + 1); put(l); },
                                Icon { name: "down" } }
                            button { "aria-label": "Remove", onclick: move |_| { let mut l = gone.clone(); l.retain(|x| *x != id2); put(l); },
                                Icon { name: "minus" } }
                        }
                    }
                }
            }
            if !spare.is_empty() {
                div { class: "chips",
                    for a in spare {
                        {
                            let mut l = quick.clone();
                            l.push(a.id.clone());
                            rsx! {
                                button { key: "{a.id}", class: "chip", onclick: move |_| put(l.clone()),
                                    Icon { name: "plus" } "{a.name}"
                                }
                            }
                        }
                    }
                }
            } else if actions.is_empty() {
                p { class: "why", "Asking the computer what it can do…" }
            }
        }
        div { class: "card",
            div { class: "cardhead", "Phone gestures" }
            for (g, label) in GESTURES.iter() {
                {
                    let now = t.gestures.get(*g).cloned().unwrap_or_default();
                    rsx! {
                        label { key: "{g}", class: "field",
                            span { "{label}" }
                            select {
                                value: "{now}",
                                onchange: move |e| set(&format!("gestures.{g}"), e.value()),
                                option { value: "", "Nothing" }
                                for a in actions.iter() { option { key: "{a.id}", value: "{a.id}", "{a.name}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}
