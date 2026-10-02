//! Connection: how this host is reached, and the hosts this device knows.

use dioxus::prelude::*;

use super::opts;
use crate::{profile, state::Ui, ui::widgets::Seg};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let relay = (s.conn_mode)() == "relay";
    let on = (ui.live.session_on)();
    let i = (s.profile)();
    let name = s
        .profiles
        .read()
        .get(i)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let host = s
        .profiles
        .read()
        .get(i)
        .map(|p| p.host.clone())
        .filter(|h| !h.is_empty());
    let redial = move || {
        crate::bridge::call(format!(
            "window.__wado.relayDial({}, {});",
            crate::bridge::js(&(s.relay_url)()),
            crate::bridge::js(&(s.remote_id)())
        ))
    };

    rsx! {
        div { class: "card",
            label { class: "field",
                span { "Name" }
                input {
                    r#type: "text", value: "{name}",
                    placeholder: "{host.clone().unwrap_or_else(|| profile::default_name(ui))}",
                    oninput: move |e| {
                        if let Some(p) = s.profiles.write().get_mut(i) { p.name = e.value(); }
                    },
                }
            }
            div { class: "cardhead", "Reached" }
            Seg {
                options: opts(&[("relay", "Via relay"), ("direct", "Direct")]),
                value: (s.conn_mode)(),
                disabled: on,
                onpick: move |v| s.conn_mode.set(v),
            }
            if relay {
                label { class: "field",
                    span { "Remote ID" }
                    input {
                        r#type: "text", inputmode: "numeric", disabled: on,
                        placeholder: "528-491-307 — shown in the daemon's log",
                        value: "{(s.remote_id)()}",
                        oninput: move |e| s.remote_id.set(e.value()),
                        onchange: move |_| redial(),
                    }
                }
                label { class: "field",
                    span { "Relay" }
                    input {
                        r#type: "url", disabled: on, placeholder: "wss://relay.example",
                        value: "{(s.relay_url)()}",
                        oninput: move |e| s.relay_url.set(e.value()),
                        onchange: move |_| redial(),
                    }
                }
            } else {
                label { class: "field",
                    span { "Address" }
                    input {
                        r#type: "url", disabled: on,
                        value: "{(s.server_addr)()}",
                        oninput: move |e| s.server_addr.set(e.value()),
                    }
                }
                p { class: "why", "Direct mode reaches a daemon on this network over HTTP. Shells and host settings need the relay." }
            }
            if on { p { class: "why", "End the session to change where it connects." } }
        }
    }
}
