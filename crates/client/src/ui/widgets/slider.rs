//! The thick slider: a native range input on top (so keyboard, screen reader and the finger all
//! work for free), with the fill and the thumb drawn as their own layers under it.
//!
//! Drawn layers rather than the input's own thumb because the native one cannot move on the
//! compositor or be told how to move: here the fill is a `scale` and the thumb a `translate`, so
//! a tap-to-jump or an arrow key **glides** on the spring, and a drag **tracks the finger 1:1**
//! with no easing at all (DESIGN.md's direct-manipulation exception). Pressing it grows the thumb
//! (`grab`) and lifts the value label, on `--pop`.
//!
//! The value shown follows the finger until the owner's value changes: a slider that sends to
//! the computer on release would otherwise snap back to the old value for the round trip.

use dioxus::prelude::*;

use super::Icon;

#[component]
pub fn Slider(
    /// What it sets — the visible label and the input's accessible name.
    label: String,
    value: f64,
    #[props(default = 0.0)] min: f64,
    #[props(default = 1.0)] max: f64,
    #[props(default = 0.02)] step: f64,
    /// An icon before the label.
    #[props(default)]
    icon: String,
    /// Draw `ticks + 1` tick marks along the bottom (a stepped slider: brightness in 10% steps).
    #[props(default)]
    ticks: u32,
    /// The 44px variant, for a slider sharing a row.
    #[props(default)]
    thin: bool,
    /// The value as words. Empty: a percentage of the range.
    #[props(default)]
    shown: String,
    /// The label stays for screen readers but is not drawn (a row that already says it).
    #[props(default)]
    quiet: bool,
    #[props(default)] disabled: bool,
    /// Every step of a drag.
    #[props(default)]
    oninput: Option<EventHandler<f64>>,
    /// Once, on release or a key.
    #[props(default)]
    onchange: Option<EventHandler<f64>>,
) -> Element {
    // (the finger's value, the owner's value when it was set)
    let mut local = use_signal(|| None::<(f64, f64)>);
    let mut grab = use_signal(|| false);
    let mut dragging = use_signal(|| false);

    let v = match local() {
        Some((mine, base)) if base == value => mine,
        _ => value,
    };
    let span = (max - min).max(f64::EPSILON);
    let frac = ((v - min) / span).clamp(0.0, 1.0);
    let text = if shown.is_empty() {
        format!("{}%", (frac * 100.0).round())
    } else {
        shown.clone()
    };
    let mut class = String::from("slide");
    if thin {
        class.push_str(" thin");
    }
    if grab() {
        class.push_str(" grab");
    }
    if dragging() {
        class.push_str(" dragging");
    }
    let mut up = move || {
        grab.set(false);
        dragging.set(false);
    };

    rsx! {
        div { class: "{class}", style: "--v:{frac:.4}",
            if ticks > 0 {
                span { class: "ticks", "aria-hidden": "true",
                    for i in 0..=ticks { i { key: "{i}" } }
                }
            }
            span { class: "fill", "aria-hidden": "true" }
            span { class: "thumbway", "aria-hidden": "true" }
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{v}",
                disabled,
                "aria-label": "{label}",
                "aria-valuetext": "{text}",
                onpointerdown: move |_| {
                    grab.set(true);
                    dragging.set(false);
                },
                onpointermove: move |_| {
                    if *grab.peek() && !*dragging.peek() {
                        dragging.set(true);
                    }
                },
                onpointerup: move |_| up(),
                onpointercancel: move |_| up(),
                onblur: move |_| up(),
                oninput: move |e| {
                    if let Ok(x) = e.value().parse::<f64>() {
                        local.set(Some((x, value)));
                        if let Some(h) = oninput {
                            h.call(x);
                        }
                    }
                },
                onchange: move |e| {
                    if let Ok(x) = e.value().parse::<f64>() {
                        local.set(Some((x, value)));
                        if let Some(h) = onchange {
                            h.call(x);
                        }
                    }
                },
            }
            if !quiet {
                span { class: "slidelab",
                    if !icon.is_empty() { Icon { name: icon.clone() } }
                    "{label}"
                }
            }
            span { class: "slideval", "{text}" }
        }
    }
}
