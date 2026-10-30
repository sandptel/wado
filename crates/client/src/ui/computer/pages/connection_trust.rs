//! Connection & trust: how this computer is reached (its name, Remote ID, relay), what this phone
//! may do there and how to allow more, the computer's Wi-Fi (it is how the computer itself is
//! reached), what only its owner may change, and forgetting it.
//!
//! Works offline on purpose: a computer that is off still has a Remote ID to fix and can still
//! be forgotten.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{
        fetch::Fetch,
        pages::{computers, connection, host, network},
    },
};

/// What this phone may do on the computer, one line per grant, with the command for the rest.
fn grants(ui: Ui) -> Element {
    let Some(g) = (ui.live.host)().grants else {
        return rsx! {};
    };
    let rows = [
        (
            "Files",
            g.files != "none",
            "files-rw",
            match g.files.as_str() {
                "ro" => "read only",
                "rw" => "read and write",
                _ => "no",
            },
        ),
        ("Shells", g.shells, "shells", ""),
        ("Sound, media, brightness, Bluetooth", g.host, "host", ""),
        ("Trackpad and typing", g.input, "input", ""),
        ("Clipboard history", g.clipboard, "clipboard", ""),
        ("Settings and Wi-Fi", g.settings, "settings", ""),
    ];
    let missing: Vec<&str> = rows.iter().filter(|r| !r.1).map(|r| r.2).collect();
    rsx! {
        div { class: "card",
            div { class: "cardhead", "This phone may" }
            for (label, yes, token, note) in rows {
                div { key: "{token}", class: "grantrow",
                    span { class: if yes { "gdot yes" } else { "gdot" } }
                    span { class: "gl", "{label}" }
                    small { if !note.is_empty() && yes { "{note}" } else if yes { "yes" } else { "no" } }
                }
            }
            if !missing.is_empty() {
                p { class: "why", "To allow more, run on the computer: " span { class: "mono", "wado allow {g.name} {missing.join(\" \")}" } " — or tick them in " span { class: "mono", "wado tui" } "." }
            }
        }
    }
}

pub fn render(ui: Ui) -> Element {
    let info = (ui.live.hoststate)()
        .map(|h| h.info)
        .filter(|i| !i.hostname.is_empty());
    rsx! {
        {connection::render(ui)}
        {grants(ui)}
        {crate::ui::scoped(ui, "push", crate::ui::push::render)}
        {crate::ui::passkey::card::render((ui.live.host)().grants.map(|g| crate::ui::passkey::card::typed(&g.name)).unwrap_or_else(|| "<this device>".into()))}
        if host::relay_only(ui) {
            div { class: "sect", span { "Its Wi-Fi" } }
            {crate::ui::scoped(ui, "wifi", network::wifi)}
            {host::notes(ui)}
            {crate::ui::scoped(ui, "owner_only", host::owner_only)}
        }
        if let Some(info) = info {
            Fetch { info }
        }
        {crate::ui::scoped(ui, "forget", computers::forget)}
    }
}
