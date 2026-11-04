//! The ? in a session (the app's `hints/cheat_sheet.dart`): what every gesture does *here*, built
//! from this phone's real settings — raw touch, edge swipes, the computer's three-finger
//! mappings — so it never describes a gesture that is off. In both arrangements; tap to close.

use dioxus::prelude::*;

use crate::state::Ui;

fn action(a: &str) -> String {
    match a {
        "app-drawer" => "your apps".into(),
        "control-centre" => "the controls".into(),
        "back" => "Back".into(),
        "focus-next" => "the next window".into(),
        "keyboard" => "the keyboard".into(),
        o => o.replace('-', " "),
    }
}

/// (fingers, words) for the picture.
fn gestures(ui: Ui) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    if (ui.set.touch_mode)() == "touch" {
        out.push((1, "Touches go to the app as touches".to_string()));
    } else {
        out.push((1, "Tap to click; drag one finger to scroll".into()));
        out.push((1, "Hold for a right-click, or to drag".into()));
        out.push((2, "Pinch with two fingers to zoom".into()));
    }
    let custom = (ui.live.host)().gestures;
    for (d, dflt) in [
        ("up", "app-drawer"),
        ("down", "control-centre"),
        ("left", "back"),
        ("right", "focus-next"),
    ] {
        let a = custom
            .get(&format!("swipe-3-{d}"))
            .map_or(dflt, String::as_str);
        if a != "none" {
            out.push((3, format!("Three fingers {d}: {}", action(a))));
        }
    }
    if (ui.set.edge_swipe)() {
        out.push((1, "Swipe in from a side edge: the next workspace".into()));
    }
    out
}

pub fn render(ui: Ui) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        button {
            id: "helpq",
            class: "helpq",
            "aria-label": "What can I do here?",
            "aria-expanded": "{open()}",
            onclick: move |_| open.toggle(),
            "?"
        }
        if open() {
            div {
                class: "helpov",
                role: "dialog",
                "aria-label": "What you can do here",
                onclick: move |_| open.set(false),
                div { class: "helpbox",
                    b { "What you can do here" }
                    p { class: "why", "Tap anywhere to close. It's always under ?." }
                    for (i, (n, words)) in gestures(ui).into_iter().enumerate() {
                        div { key: "{i}", class: "helprow",
                            span { class: "fingers", for k in 0..n { i { key: "{k}" } } }
                            span { "{words}" }
                        }
                    }
                }
            }
        }
    }
}
