//! Add a computer: its Remote ID, the relay it is on (picked from the ones this phone knows, or
//! a new one), and a name if wanted. The usual way in is still its QR code or link; this is
//! for typing one.

use dioxus::prelude::*;

use crate::{
    profile::Profile,
    route,
    state::Ui,
    ui::{
        pages::computers::{self, known_relays},
        widgets::Icon,
    },
};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let relays = known_relays(ui);
    let mut name = use_signal(String::new);
    let mut id = use_signal(String::new);
    let first = relays.first().cloned().unwrap_or_default();
    let mut relay = use_signal(move || first);
    let digits = id().chars().filter(char::is_ascii_digit).count();
    let ready = digits >= 6 && !relay().is_empty();

    rsx! {
        div { class: "card",
            p { class: "why", "Scanning the computer's QR code (" span { class: "mono", "wado qr" } " there) is quicker — it brings everything below with it." }
            label { class: "field",
                span { "Remote ID" }
                input {
                    r#type: "text", inputmode: "numeric", placeholder: "528-491-307 — shown by wado on the computer",
                    value: "{id}", oninput: move |e| id.set(e.value()),
                }
            }
            label { class: "field",
                span { "Relay" }
                select {
                    value: "{relay}",
                    onchange: move |e| relay.set(e.value()),
                    for r in relays.iter().cloned() { option { key: "{r}", value: "{r}", "{computers::short(&r)}" } }
                }
            }
            label { class: "field",
                span { "Name" }
                input { r#type: "text", placeholder: "Named after the computer once it answers", value: "{name}", oninput: move |e| name.set(e.value()) }
            }
            button {
                class: "btn primary",
                disabled: !ready,
                onclick: move |_| {
                    let p = Profile {
                        name: name().trim().to_string(),
                        conn_mode: "relay".into(),
                        relay_url: relay(),
                        remote_id: id().trim().to_string(),
                        ..Default::default()
                    };
                    let key = p.key();
                    s.profiles.write().push(p);
                    route::open_computer(ui, &key);
                },
                Icon { name: "plus" } "Add it"
            }
        }
        {crate::ui::scoped(ui, "relays", computers::relays)}
    }
}
