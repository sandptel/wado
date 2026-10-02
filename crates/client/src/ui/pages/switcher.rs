//! Workspaces & windows: how windows are laid out, and the chrome that manages them — the
//! workspace bar, the quick rail, the dock's fade and edge swipes.

use dioxus::prelude::*;

use super::opts;
use crate::{
    actions,
    state::Ui,
    ui::{
        live::apply,
        widgets::{Seg, SwitchRow},
    },
};

/// The layouts, as the settings value and what the viewer calls it.
const LAYOUTS: &[(&str, &str)] = &[
    ("auto", "Auto"),
    ("strip", "Sliding"),
    ("tile", "Tiling"),
    ("maximized", "Full"),
    ("center", "Floating"),
];

pub fn layout_name(v: &str) -> &'static str {
    match v {
        "cascade" | "top_left" => "Floating",
        _ => LAYOUTS
            .iter()
            .find(|(k, _)| *k == v)
            .map_or("Auto", |(_, n)| n),
    }
}

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let on = (ui.live.session_on)();
    rsx! {
        div { class: "card",
            div { class: "cardhead", "Layout" }
            Seg {
                options: opts(LAYOUTS),
                value: (s.placement)(),
                // Applied at once when a session runs: a layout is something to try and see.
                onpick: move |v| { s.placement.set(v); if on { actions::apply(ui); } },
            }
            p { class: "hint",
                match (s.placement)().as_str() {
                    "strip" => "Columns side by side; swipe the strip to move between them.",
                    "tile" => "Every window on the workspace shares the screen.",
                    "maximized" => "Each window fills the screen; the workspace bar switches.",
                    "center" | "cascade" | "top_left" => "Windows float where they open; drag them by the title.",
                    _ => "Sliding on a phone, floating on a big screen.",
                }
            }
        }
        div { class: "card",
            SwitchRow {
                title: "Workspace bar",
                sub: "Numbered workspaces and their apps, left of the dock",
                on: (s.ws_bar)(),
                ontoggle: move |v| s.ws_bar.set(v),
            }
            SwitchRow {
                title: "Pin the dock",
                sub: "Keep it on screen instead of fading after 2 s",
                on: (s.dock_pin)(),
                ontoggle: move |v| { s.dock_pin.set(v); apply(ui); },
            }
            SwitchRow {
                title: "Edge swipes",
                sub: "Swipe in from a side edge for the next workspace",
                on: (s.edge_swipe)(),
                ontoggle: move |v| { s.edge_swipe.set(v); apply(ui); },
            }
            div { class: "cardhead", "Quick rail" }
            Seg {
                options: opts(&[("left", "Left"), ("right", "Right"), ("off", "Off")]),
                value: (s.rail)(),
                onpick: move |v| s.rail.set(v),
            }
            p { class: "hint", "Rotate, fullscreen and the keyboard, on the screen's edge." }
        }
        div { class: "card",
            div { class: "cardhead", "Moving windows" }
            p { class: "hint",
                "Hold an app's icon in the workspace bar: move it to another workspace, or to a new one. "
                "Tap a number to switch; + opens an empty workspace."
            }
        }
    }
}
