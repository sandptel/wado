//! Input & touch: what a finger, a wheel and a key are to the session.

use dioxus::prelude::*;

use super::opts;
use crate::{
    state::Ui,
    ui::{
        live::apply,
        widgets::{Seg, SwitchRow, When, WhenBadge},
    },
};

const GESTURE_ACTIONS: &[(&str, &str)] = &[
    ("app-drawer", "App drawer"),
    ("control-centre", "Control centre"),
    ("keyboard", "Keyboard"),
    ("back", "Back"),
    ("focus-next", "Next window"),
    ("maximize", "Maximize"),
    ("minimize", "Minimize"),
    ("close-window", "Close window"),
    ("none", "Nothing"),
];

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let host = (ui.live.host)();
    let pinned = host.input.clone();
    let gestures = host.gestures.clone();

    rsx! {
        div { class: "card",
            div { class: "cardhead", "A finger is" }
            Seg {
                options: opts(&[("pointer", "Gestures"), ("touch", "Raw touch")]),
                value: (s.touch_mode)(),
                onpick: move |v| { s.touch_mode.set(v); apply(ui); },
            }
            p { class: "why",
                if (s.touch_mode)() == "touch" { "Real multi-touch, for touch-native apps." }
                else { "Tap clicks, hold right-clicks, drag scrolls." }
            }
            SwitchRow {
                title: "Move windows",
                sub: "Dragging moves the window under the finger",
                on: (s.move_mode)(),
                ontoggle: move |on| { s.move_mode.set(on); apply(ui); },
            }
        }

        div { class: "card",
            div { class: "slide",
                input {
                    r#type: "range", min: "0.05", max: "2", step: "0.05", "aria-label": "Scroll speed",
                    value: "{(s.scroll_speed)()}",
                    oninput: move |e| if let Ok(v) = e.value().parse::<f64>() { s.scroll_speed.set(v); apply(ui); },
                }
                span { class: "slidelab", "Scroll speed" }
                span { class: "slideval", "{(s.scroll_speed)():.2}×" }
            }
            SwitchRow {
                title: "Natural scroll",
                sub: "Content follows the finger",
                on: (s.natural_scroll)(),
                ontoggle: move |on| { s.natural_scroll.set(on); apply(ui); },
            }
        }

        div { class: "card",
            div { class: "cardhead", "Three-finger swipes" WhenBadge { when: When::Host } }
            for dir in ["up", "down", "left", "right"] {
                {
                    let key = format!("swipe-3-{dir}");
                    let now = gestures.get(&key).cloned().unwrap_or_default();
                    rsx! {
                        label { key: "{key}", class: "field",
                            span { "Swipe {dir}" }
                            select {
                                value: "{now}",
                                onchange: move |e| crate::bridge::call(format!(
                                    "window.__wado.configSet({}, {}, false);",
                                    crate::bridge::js(&format!("gestures.{key}")),
                                    crate::bridge::js(&e.value())
                                )),
                                for (v, l) in GESTURE_ACTIONS { option { key: "{v}", value: "{v}", "{l}" } }
                            }
                        }
                    }
                }
            }
            p { class: "why", "Saved on this computer for every device. Gesture mode only — raw touch passes three fingers to the app." }
        }

        div { class: "card",
            div { class: "cardhead", "Keyboard repeat" WhenBadge { when: if pinned.repeat_rate.is_some() || pinned.repeat_delay.is_some() { When::Host } else { When::Apply } } }
            div { class: "row",
                label { class: "field",
                    span { "keys / s" }
                    input {
                        r#type: "number", min: "1",
                        disabled: pinned.repeat_rate.is_some(),
                        value: "{pinned.repeat_rate.unwrap_or((s.repeat_rate)())}",
                        oninput: move |e| if let Ok(v) = e.value().parse() { s.repeat_rate.set(v) },
                    }
                }
                label { class: "field",
                    span { "delay ms" }
                    input {
                        r#type: "number", min: "0",
                        disabled: pinned.repeat_delay.is_some(),
                        value: "{pinned.repeat_delay.unwrap_or((s.repeat_delay)())}",
                        oninput: move |e| if let Ok(v) = e.value().parse() { s.repeat_delay.set(v) },
                    }
                }
            }
            SwitchRow {
                title: "Focus follows pointer",
                sub: "Hovering a window focuses it",
                on: pinned.focus_follows_pointer.unwrap_or((s.focus_follows)()),
                disabled: pinned.focus_follows_pointer.is_some(),
                when: if pinned.focus_follows_pointer.is_some() { When::Host } else { When::Apply },
                ontoggle: move |on| s.focus_follows.set(on),
            }
        }
    }
}
