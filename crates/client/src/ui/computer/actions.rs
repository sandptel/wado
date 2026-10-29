//! The four reasons to open a computer: its desktop, this phone as its trackpad, its files, a
//! shell. Big tiles, Desktop first and loudest — it is the one most taps are for.
//!
//! Each says what it will do *now* ("Rejoin · Firefox and 2 more", "Start at this screen's
//! size"), and a door this phone may not use says so instead of failing after the tap.

use dioxus::prelude::*;

use crate::{
    bridge,
    reach::Reach,
    state::Ui,
    ui::{home::sessions, widgets::Icon},
};

#[component]
pub fn Actions(ui: Ui, r: Reach) -> Element {
    let live = ui.live;
    let relay = (ui.set.conn_mode)() == "relay";
    let (_, list) = (live.sessions)();
    let running = !list.is_empty();
    let shells_open = (live.shells)().iter().filter(|s| s.alive).count();
    let off = !r.online;
    let grants = r.grants();
    let files_ok = grants.as_ref().is_none_or(|g| g.files != "none");
    let pad_ok = r.may("input");
    let shell_ok = r.may("shells") && r.config.as_ref().is_none_or(|c| c.shells);

    let desk_sub = if r.busy {
        "In use on another device".to_string()
    } else if running {
        let apps: Vec<String> = list
            .iter()
            .flat_map(|s| s.apps.iter())
            .map(|a| crate::ui::home::sessions::short(a))
            .collect();
        match apps.len() {
            0 => "Rejoin · empty desktop".into(),
            1 => format!("Rejoin · {}", apps[0]),
            n => format!("Rejoin · {} and {} more", apps[0], n - 1),
        }
    } else {
        "Start at this screen's size".into()
    };

    rsx! {
        section { class: "acts sx o2", style: "--n:1", "aria-label": "Open",
            button {
                class: "act primary",
                disabled: off && !r.busy,
                onclick: move |_| {
                    if r.busy {
                        bridge::call("window.__wado.relayTakeover();".to_string());
                    } else if running {
                        sessions::join(ui);
                    } else {
                        crate::actions::start(ui);
                    }
                },
                if running && !r.busy { span { class: "badge", "running" } }
                if r.busy { span { class: "badge", "take over" } }
                span { class: "aic", Icon { name: "play", fill: true } }
                span { b { "Desktop" } small { "{desk_sub}" } }
            }
            button {
                class: "act",
                disabled: off || !relay || !pad_ok,
                onclick: move |_| bridge::call("window.__wado.padOpen();".to_string()),
                span { class: "aic", Icon { name: "trackpad" } }
                span { b { "Trackpad" } small { if pad_ok { "This phone as its touchpad" } else { "Not allowed for this phone" } } }
            }
            button {
                class: "act",
                disabled: off || !relay || !files_ok,
                onclick: move |_| bridge::call("window.__wado.filesOpen();".to_string()),
                span { class: "aic", Icon { name: "folder" } }
                span { b { "Files" } small { if files_ok { "Browse, send, download" } else { "Not allowed for this phone" } } }
            }
            button {
                class: "act",
                disabled: off || !relay || !shell_ok,
                onclick: move |_| crate::ui::console::open_shells(ui),
                if shells_open > 0 { span { class: "badge", "{shells_open} open" } }
                span { class: "aic", Icon { name: "term" } }
                span { b { "Shell" } small { if shell_ok { "A terminal, no video" } else { "Not allowed for this phone" } } }
            }
        }
    }
}
