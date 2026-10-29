//! The Trackpad widget: a touch surface that opens the trackpad for its computer, and that
//! computer's quick actions (its `trackpad { quick }` list) as chips that run at a tap.
//!
//! The surface blooms a ring from the finger (`js/widget_touch.js`) — the one motion here that
//! follows a touch rather than a state.

use dioxus::prelude::*;
use wado_protocol::pad::ActionInfo;

use crate::{bridge, ui::widgets::Icon};

use super::shell::Ctx;

/// What a chip shows for an action id the computer has not listed (yet).
fn chip_icon(icon: &str) -> &str {
    match icon {
        "" => "dots",
        i => i,
    }
}

#[component]
pub fn Trackpad(cx: Ctx) -> Element {
    let ui = cx.ui;
    let pk = cx.w.host.clone();
    let (prefs, actions): (Option<wado_protocol::TrackpadPrefs>, Vec<ActionInfo>) = if cx.r.selected
    {
        ((ui.live.host)().trackpad, (ui.live.actions)())
    } else {
        let side = ui.live.side.read().get(&pk).cloned().unwrap_or_default();
        (side.config.and_then(|c| c.trackpad), side.actions)
    };
    // The selected computer's list is asked for once, when someone looks.
    let ask = cx.r.selected && actions.is_empty();
    use_hook(move || {
        if ask {
            bridge::call("window.__wado.actionsGet();".to_string())
        }
    });
    let quick = prefs.map(|p| p.quick).unwrap_or_else(|| {
        [
            "workspace-prev",
            "workspace-next",
            "fullscreen",
            "play-pause",
        ]
        .map(String::from)
        .to_vec()
    });
    let chips: Vec<ActionInfo> = quick
        .iter()
        .filter_map(|id| actions.iter().find(|a| &a.id == id).cloned())
        .collect();
    let relayed = cx.r.selected && (ui.live.link)().state == "relayed";
    let name = cx.name.clone();
    let k = pk.clone();
    rsx! {
        button {
            class: "padw",
            "aria-label": "Use {name}'s pointer — opens the trackpad",
            onclick: move |_| {
                if let Some(i) = crate::route::index_of(ui, &k) {
                    if (ui.set.profile)() != i { crate::profile::select(ui, i); }
                }
                bridge::call("window.__wado.padOpen();".to_string());
            },
            Icon { name: "trackpad" }
            span { "Touch to use {name}'s pointer"
                small { if relayed { "Relayed — plays a little behind your finger" } else { "Opens the trackpad" } }
            }
        }
        if !chips.is_empty() {
            div { class: "qchips",
                for a in chips {
                    {
                        let (id, pk) = (a.id.clone(), pk.clone());
                        let sel = cx.r.selected;
                        rsx! {
                            button { key: "{a.id}", class: "qchip",
                                onclick: move |_| bridge::call(if sel {
                                    format!("window.__wado.actionRun({});", bridge::js(&id))
                                } else {
                                    format!("window.__wado.sideDo({}, {{type: 'pad_in', msg: {{k: 'run', id: {}}}}});", bridge::js(&pk), bridge::js(&id))
                                }),
                                Icon { name: chip_icon(&a.icon).to_string() }
                                "{a.name}"
                            }
                        }
                    }
                }
            }
        }
    }
}
