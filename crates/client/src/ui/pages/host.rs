//! This computer: the daemon's own config.kdl, from the client.
//!
//! Live keys (the stream caps, pinned input) any trusted device may set. The rest needs the
//! owner device and a confirmation, and the daemon checks both — what is hidden here is a
//! courtesy, not the rule. Every change goes to `ui.kdl`; the hand-written file is not touched.

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::SwitchRow};

fn set(key: &str, value: String, confirmed: bool) {
    bridge::call(format!(
        "window.__wado.configSet({}, {}, {confirmed});",
        bridge::js(&key),
        bridge::js(&value)
    ));
}

fn num(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

#[component]
fn Cap(label: String, cfg_key: String, value: Option<u32>, unit: String) -> Element {
    rsx! {
        label { class: "field",
            span { "{label}" }
            input {
                r#type: "number", min: "1", inputmode: "numeric", placeholder: "no limit",
                value: "{num(value)}",
                // On change, not on input: each write reloads the daemon's config.
                onchange: move |e| set(&cfg_key, e.value(), false),
            }
            small { "{unit}" }
        }
    }
}

pub fn render(ui: Ui) -> Element {
    let h = (ui.live.host)();
    let note = (ui.live.config_note)();
    let relay = (ui.set.conn_mode)() == "relay";
    let mut confirm = use_signal(|| None::<(&'static str, String)>);
    let mut new_combo = use_signal(String::new);
    let mut new_action = use_signal(|| "close-window".to_string());

    if !relay {
        return rsx! { div { class: "card", p { class: "why", "Host settings travel over the relay. Connect via relay to change them here — or edit ~/.config/wado/config.kdl on that computer." } } };
    }

    let info = (ui.live.hoststate)()
        .map(|h| h.info)
        .filter(|i| !i.hostname.is_empty());

    rsx! {
        if let Some(info) = info {
            crate::ui::fetch::Fetch { info }
        }
        if let Some(err) = h.error.clone() {
            div { class: "card alertcard",
                b { "config.kdl has a mistake" }
                p { class: "mono", "{err}" }
                p { class: "why", "The daemon is still running on the last good version." }
            }
        }
        if !h.restart.is_empty() {
            div { class: "card", p { class: "why", "Restart the daemon to apply: {h.restart.join(\", \")}" } }
        }
        if !note.is_empty() {
            div { class: "card alertcard", p { "{note}" } }
        }

        div { class: "card",
            div { class: "cardhead", "Stream caps" }
            p { class: "why", "Every device's session is held inside these. Empty means no limit." }
            Cap { label: "Frame rate", cfg_key: "stream.max-fps", value: h.limits.max_fps, unit: "fps" }
            Cap { label: "Bitrate", cfg_key: "stream.max-bitrate", value: h.limits.max_bitrate, unit: "kbps" }
            Cap { label: "Width", cfg_key: "stream.max-width", value: h.limits.max_width, unit: "px" }
            Cap { label: "Height", cfg_key: "stream.max-height", value: h.limits.max_height, unit: "px" }
            label { class: "field",
                span { "Encoder" }
                select {
                    value: match h.limits.encoder {
                        Some(wado_protocol::EncoderBackend::Hardware) => "hardware",
                        Some(wado_protocol::EncoderBackend::Software) => "software",
                        _ => "auto",
                    },
                    onchange: move |e| set("stream.encoder", e.value(), false),
                    option { value: "auto", "Each device chooses" }
                    option { value: "hardware", "Always GPU" }
                    option { value: "software", "Always x264" }
                }
            }
        }

        div { class: "card",
            div { class: "cardhead", "Input for everyone" }
            SwitchRow {
                title: "Pin focus-follows-pointer",
                sub: if h.input.focus_follows_pointer.is_some() { "Pinned for every device" } else { "Each device chooses" },
                on: h.input.focus_follows_pointer.is_some(),
                ontoggle: move |v: bool| set("input.focus-follows-pointer", if v { "true".into() } else { String::new() }, false),
            }
        }

        div { class: "card",
            div { class: "cardhead", "Shortcuts" }
            p { class: "why", "Keys the compositor keeps for itself. Mod is {h.bind_mod} — a browser cannot always capture Super, so ctrl+alt is the fallback." }
            label { class: "field",
                span { "Mod is" }
                select {
                    value: "{h.bind_mod}",
                    onchange: move |e| set("binds.mod", e.value(), false),
                    for m in ["super", "alt", "ctrl", "ctrl+alt"] { option { key: "{m}", value: "{m}", "{m}" } }
                }
            }
            for (combo, action) in h.binds.clone().into_iter() {
                div { key: "{combo}", class: "editrow",
                    b { class: "mono", "{combo}" }
                    span { class: "why", "{action}" }
                    button { "aria-label": "Remove {combo}", onclick: move |_| set(&format!("binds.{combo}"), String::new(), false),
                        crate::ui::widgets::Icon { name: "x" }
                    }
                }
            }
            div { class: "row",
                input { r#type: "text", placeholder: "Mod+Q", value: "{new_combo}", oninput: move |e| new_combo.set(e.value()) }
                select {
                    value: "{new_action}",
                    onchange: move |e| new_action.set(e.value()),
                    for a in ["close-window", "maximize", "minimize", "focus-next", "back"] { option { key: "{a}", value: "{a}", "{a}" } }
                }
            }
            button {
                class: "btn",
                disabled: new_combo().trim().is_empty(),
                onclick: move |_| {
                    set(&format!("binds.{}", new_combo().trim()), new_action(), false);
                    new_combo.set(String::new());
                },
                "Add shortcut"
            }
            if h.window_rules > 0 {
                p { class: "why", "{h.window_rules} window rule(s) in config.kdl." }
            }
        }

        div { class: "card",
            div { class: "cardhead", "Owner only" }
            if h.owner {
                p { class: "why", "These run code or change who can connect. Each asks first." }
                SwitchRow {
                    title: "Shells",
                    sub: "Allow terminals on this computer",
                    on: h.shells,
                    ontoggle: move |v: bool| confirm.set(Some(("shells.enabled", v.to_string()))),
                }
                if let Some((key, value)) = confirm() {
                    div { class: "confirm",
                        p { "Change {key} to {value} on this computer?" }
                        div { class: "actions",
                            button { class: "btn", onclick: move |_| confirm.set(None), "Cancel" }
                            button { class: "btn primary", onclick: move |_| { set(key, value.clone(), true); confirm.set(None); }, "Change it" }
                        }
                    }
                }
            } else {
                p { class: "why", "Shells, autostart, environment and trust can only be changed from the owner device — the first one trusted — or in config.kdl." }
            }
        }
    }
}
