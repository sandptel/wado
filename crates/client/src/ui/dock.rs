//! The dock, spread into three groups along the bottom edge: the **workspace bar** on the left,
//! **⊞ Apps · ○ Home · ◁ Back** in the middle where Android keeps them, **⋯ Control centre** on
//! the right. ⋯ becomes ⌨ for the one moment that is worth a slot: an app wants typing and the
//! keyboard is down. The **quick rail** on a side edge holds rotate, fullscreen and keyboard.
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

    let s = ui.set;
    let c = crate::cfg::build(ui);
    let turn_to = if c.width >= c.height {
        "portrait"
    } else {
        "landscape"
    };
    let rail = (s.rail)();
    let bar_class = if cc {
        "hidden"
    } else if (live.recording)() {
        "rec"
    } else {
        ""
    };

    rsx! {
        // No `idle` class here: the dock starts visible and js/bar.js fades it on a timer.
        nav { id: "wado-bar", class: bar_class, "aria-label": "Session",
            div { class: "grp left",
                if (s.ws_bar)() { {crate::ui::workspaces::bar(ui)} }
            }
            div { class: "grp mid",
                button {
                    class: "barbtn nav",
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
                    Icon { name: "apps" }
                }
                // Home: the first empty workspace (compositor `workspace.rs`). Again goes back.
                button {
                    class: "barbtn nav home",
                    title: "Home",
                    "aria-label": "Home — an empty workspace",
                    onclick: move |_| bridge::call("window.__wado.windowAction(\"home\");".to_string()),
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
            div { class: "grp right",
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
            }
        }
        // The quick rail: rotate, fullscreen, keyboard on the screen's edge. Rotate and
        // fullscreen are caught by js/chrome.js inside the tap — both need the user gesture.
        if rail != "off" {
            div { class: "rail {rail}", "aria-label": "Quick",
                button { class: "barbtn", "data-rotate": turn_to, title: "Turn to {turn_to}", "aria-label": "Turn to {turn_to}",
                    Icon { name: "rotate" }
                }
                button { class: "barbtn", "data-fullscreen": "1", title: "Fullscreen", "aria-label": "Fullscreen",
                    Icon { name: "max" }
                }
                label { r#for: "wado-osk", class: "barbtn", title: "Keyboard", "aria-label": "Keyboard",
                    Icon { name: "kbd" }
                }
            }
        }
        {crate::ui::workspaces::sheet(ui)}
    }
}
