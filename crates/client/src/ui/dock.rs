//! The dock: three buttons, Android-style — **⋯ Control centre · ○ Apps · ◁ Back** — so Back
//! sits under a right thumb (Decision Log 2026-09-29). ⋯ becomes ⌨ for the one moment that is
//! worth a slot: an app wants typing and the keyboard is down.
//!
//! The 6 px LED on ⋯ is the only status always on screen. It is how invariant #5 stays
//! visible without a banner reflowing the picture: software encoding turns it orange, and it
//! cannot be switched off. Floats over the video and fades when idle (`js/bar.js`).

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::Icon};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let on = (live.session_on)();
    if !on {
        return rsx! {};
    }
    let type_now = (live.text_wanted)() && !(live.osk_on)();
    let cc = (live.cc_open)();
    let led = if (live.encoder_mode)() == "software" {
        "sw"
    } else {
        match (live.health)().state.as_str() {
            "" | "ok" => "ok",
            s => {
                if s == "bad" {
                    "bad"
                } else {
                    "warn"
                }
            }
        }
    };

    rsx! {
        // No `idle` class here: the dock starts visible and js/bar.js fades it on a timer.
        nav { id: "wado-bar", class: if cc { "hidden" } else { "" }, "aria-label": "Session",
            if type_now {
                // A `label`, not a `button`: Android raises the soft keyboard only for a focus
                // inside the user gesture, and label activation focuses its target natively.
                label { r#for: "wado-osk", class: "barbtn nav", title: "Keyboard", "aria-label": "Keyboard",
                    Icon { name: "kbd" }
                }
            } else {
                button {
                    class: "barbtn nav more led-{led}",
                    title: "Control centre",
                    "aria-label": "Control centre",
                    "aria-expanded": "{cc}",
                    onclick: move |_| live.cc_open.set(true),
                    Icon { name: "dots" }
                }
            }
            button {
                class: "barbtn nav home",
                title: "Apps",
                "aria-label": "Apps",
                onclick: move |_| {
                    let open = !(live.drawer_open)();
                    live.drawer_open.set(open);
                    // Re-asked on every open: the list says which apps are running *now*.
                    if open {
                        bridge::call("window.__wado.requestApps();".to_string());
                    }
                },
                span { class: "homering" }
            }
            button {
                class: "barbtn nav",
                title: "Back",
                "aria-label": "Back",
                onclick: move |_| bridge::call("window.__wado.back();".to_string()),
                Icon { name: "back" }
            }
        }
    }
}
