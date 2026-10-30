//! Size: this device's text size and interface size (DESIGN.md, "Size"). Its own, like the
//! console's preferences: written to `device { size { … } }` on the computer by `js/ui_size.js`,
//! which also applies them, so they are not in the settings blob.
//!
//! The text slider previews in its sample line while it is dragged and applies on release: the
//! whole page re-flowing under the finger would move the slider away from it.

use dioxus::prelude::*;
use serde_json::json;

use super::opts;
use crate::{
    size::{self, SIZE, TEXT_MAX, TEXT_MIN, TEXT_STEP},
    ui::widgets::{Seg, Slider, SwitchRow},
};

#[component]
pub fn SizeCards() -> Element {
    let s = SIZE();
    // The size being dragged to, previewed in the sample only.
    let mut drag = use_signal(|| None::<u32>);
    let shown = drag().unwrap_or(s.effective_text());
    let native = s.device.is_some();
    let follow_title = if native {
        "Follow phone"
    } else {
        "Follow this browser"
    };
    let follow_sub = match (native, s.device) {
        (true, Some(d)) => format!(
            "Use the phone's own font size, now {}%",
            (d * 100.0).round()
        ),
        _ => "Use the browser's own default text size".to_string(),
    };
    let text = s.text;
    let iface = s.interface.clone();

    rsx! {
        div { class: "card sizecard",
            div { class: "cardhead", "Text size" }
            Slider {
                label: "Text size",
                quiet: true,
                value: f64::from(if s.follow { shown } else { text }),
                min: f64::from(TEXT_MIN),
                max: f64::from(TEXT_MAX),
                step: f64::from(TEXT_STEP),
                ticks: (TEXT_MAX - TEXT_MIN) / TEXT_STEP,
                shown: format!("{shown}%"),
                disabled: s.follow,
                oninput: move |v: f64| drag.set(Some(v.round() as u32)),
                onchange: move |v: f64| {
                    drag.set(None);
                    size::set("text", json!(v.round() as u32));
                },
            }
            div { class: "sizestep",
                button {
                    class: "btn",
                    "aria-label": "Smaller text",
                    disabled: s.follow || text <= TEXT_MIN,
                    onclick: move |_| size::set("text", json!(size::step(text, false))),
                    span { class: "sizea small", "aria-hidden": "true", "A" }
                    "Smaller"
                }
                button {
                    class: "btn",
                    "aria-label": "Larger text",
                    disabled: s.follow || text >= TEXT_MAX,
                    onclick: move |_| size::set("text", json!(size::step(text, true))),
                    span { class: "sizea", "aria-hidden": "true", "A" }
                    "Larger"
                }
            }
            div { class: "sizesample", style: "--p:{f64::from(shown) / 100.0}", "aria-hidden": "true",
                b { "Living-room PC" }
                span { "Connected over your network. Text reads like this." }
                small { "NixOS · Hyprland · 1080 × 2400" }
            }
            SwitchRow {
                title: follow_title,
                sub: follow_sub,
                on: s.follow,
                ontoggle: move |on: bool| size::set("follow", json!(on)),
            }
            if text != 100 && !s.follow {
                button { class: "sizereset", onclick: move |_| size::set("text", json!(100)), "Back to 100%" }
            }
        }

        div { class: "card",
            div { class: "cardhead", "Interface size" }
            Seg {
                options: opts(size::INTERFACES),
                value: iface.clone(),
                onpick: move |v: String| size::set("interface", json!(v)),
            }
            p { class: "hint",
                match iface.as_str() {
                    "compact" => "Smaller controls and tighter spacing: more on the screen at once.",
                    "comfortable" => "Roomier controls and spacing, and the text grows with them.",
                    "large" => "The largest controls and spacing, and the text grows with them.",
                    _ => "The size wado is designed at. Controls, spacing and text grow or shrink together.",
                }
            }
        }
    }
}
