//! The `?` (the app's `hints/cheat_sheet.dart`): what every gesture or control does *here* — the
//! shared rows (`wado_ux::help`), from this device's real settings and its form: touch rows on a
//! phone or tablet, the mouse, the keys and the computer's own shortcuts on a desktop. In a session
//! in both arrangements, and on home. Tap or Esc to close.

use dioxus::prelude::*;

use crate::{state::Ui, ui::widgets::Icon};

fn rows(ui: Ui, surface: &str) -> Vec<wado_ux::help::Row> {
    let host = (ui.live.host)();
    wado_ux::help::rows(&wado_ux::help::Input {
        surface: surface.into(),
        form: crate::ui::sheet::form(ui),
        raw: (ui.set.touch_mode)() == "touch",
        // The web has no per-game swipe switch: Game mode's calm is the app's.
        swipe3: true,
        edge_swipe: (ui.set.edge_swipe)(),
        game: (ui.live.game)().is_some(),
        gestures: host.gestures,
        binds: host.binds,
        bind_mod: host.bind_mod,
    })
}

/// The overlay for `surface` (`home`, `picture`, `pad`).
pub fn overlay(ui: Ui, surface: &str, close: impl FnMut() + Clone + 'static) -> Element {
    let list = rows(ui, surface);
    let c1 = close.clone();
    let mut c2 = close;
    rsx! {
        div {
            class: "helpov",
            role: "dialog",
            "aria-label": "What you can do here",
            tabindex: "-1",
            onmounted: move |e| { let _ = e.data().set_focus(true); },
            onclick: move |_| { let mut c = c1.clone(); c() },
            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { c2() },
            div { class: "helpbox",
                b { "What you can do here" }
                p { class: "why", "Tap anywhere to close. It's always under ?." }
                for (i, r) in list.into_iter().enumerate() {
                    div { key: "{i}", class: "helprow", style: "--i: {i}",
                        span { class: "fingers",
                            for k in 0..r.fingers { i { key: "{k}" } }
                            if !r.icon.is_empty() { Icon { name: r.icon.clone() } }
                        }
                        span { "{r.words}" }
                    }
                }
            }
        }
    }
}

/// The session's `?`, beside the link badge in both arrangements.
pub fn render(ui: Ui) -> Element {
    let mut open = use_signal(|| false);
    let face = (ui.live.stage)().face();
    let surface = if face == crate::ui::surround::layout::Face::Gamepad { "pad" } else { "picture" };
    rsx! {
        button {
            id: "helpq",
            class: "helpq",
            "aria-label": "What can I do here?",
            "aria-expanded": "{open()}",
            onclick: move |_| { crate::ui::tips::mark(ui, "help"); open.toggle() },
            "?"
        }
        if !open() {
            crate::ui::tips::Tip { ui, id: "help".to_string(), title: "Click or tap ? any time".to_string(), words: "It shows what every gesture, key and button does here.".to_string() }
        }
        if open() {
            {overlay(ui, surface, move || open.set(false))}
        }
    }
}
