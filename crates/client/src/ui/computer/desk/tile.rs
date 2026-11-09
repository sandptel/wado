//! One app's icon, and its menu.
//!
//! A tap acts at once ([`super::tap`]); a long press (a timer in `js/desk_apps.js`, as iOS never
//! sends `contextmenu`) or a right-click opens the menu, the Dock's: what it is and its state,
//! then Open or Show, Hide or Show, Quit, Force Quit — which asks first.
//!
//! State is drawn, never colour alone: a dot under it when it runs (the accent, and lifted, when
//! it is in front), dimmed when hidden, a bounce while it starts; and said in its label.

use dioxus::prelude::*;
use wado_protocol::AppEntry;

use super::{act, items, letter, state, Menu as Open, LAUNCHING, MENU};
use crate::{state::Ui, ui::sheet::Sheet};

/// `named`: the name under it (the Apps page); in the dock it is the tooltip only.
#[component]
pub fn Tile(app: AppEntry, named: bool) -> Element {
    let launching = !app.running && LAUNCHING.read().contains(&app.exec);
    let class = format!(
        "dtile{}{}{}{}",
        if app.running { " run" } else { "" },
        if app.front { " front" } else { "" },
        if app.running && app.away { " away" } else { "" },
        if launching { " launching" } else { "" },
    );
    let label = format!("{}, {}", app.name, super::state(&app).to_lowercase());
    let (exec, action, running) = (app.exec.clone(), super::tap(&app), app.running);
    rsx! {
        button {
            class: "{class}",
            "data-dapp": "{app.exec}",
            "aria-label": "{label}",
            title: if named { "" } else { "{app.name}" },
            onclick: move |_| act(&exec, action, running),
            span { class: "dicon",
                if let Some(src) = app.icon.as_ref() {
                    img { src: "{src}", alt: "", draggable: "false" }
                } else {
                    span { class: "dletter", "{letter(&app.name)}" }
                }
            }
            i { class: "ddot", "aria-hidden": "true" }
            if named {
                span { class: "dname", "{app.name}" }
            }
        }
    }
}

/// The menu of the app [`MENU`] names, while it is in the list.
#[component]
pub fn Menu(ui: Ui) -> Element {
    let Some(m) = MENU() else {
        return rsx! {};
    };
    let Some(app) = ui
        .live
        .apps
        .read()
        .iter()
        .find(|a| a.exec == m.exec)
        .cloned()
    else {
        return rsx! {};
    };
    let close = move || *MENU.write() = None;
    let (exec, running) = (app.exec.clone(), app.running);
    rsx! {
        Sheet {
            title: app.name.clone(),
            sub: Some(state(&app).to_string()),
            icon: app.icon.clone(),
            on_close: move |_| close(),
            if m.confirm {
                p { class: "why dconfirm", "Unsaved changes in {app.name} will be lost." }
                button {
                    class: "btn danger",
                    onclick: { let exec = exec.clone(); move |_| { act(&exec, "force_quit", true); close(); } },
                    "Force Quit"
                }
                button { class: "btn", onclick: move |_| close(), "Cancel" }
            } else {
                div { class: "gpicklist dmenu",
                    for (label, action) in items(&app) {
                        button {
                            key: "{action}",
                            class: if action == "force_quit" { "gpickrow bad" } else { "gpickrow" },
                            onclick: {
                                let exec = exec.clone();
                                move |_| if action == "force_quit" {
                                    *MENU.write() = Some(Open { exec: exec.clone(), confirm: true });
                                } else {
                                    act(&exec, action, running);
                                    close();
                                }
                            },
                            span { "{label}" }
                        }
                    }
                }
            }
        }
    }
}
