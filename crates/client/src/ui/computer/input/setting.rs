//! One setting, described by a spec and drawn as its control: a switch, a thick slider, a
//! segmented choice, or a choice of pictures. The input pages are tables of these
//! (`touchpad::SECTIONS`), so a new setting — a tap threshold, a gesture's sensitivity — is one
//! more line in a table and one more field in the device's block, not a new piece of page.
//!
//! The value comes from the settings model as JSON (the bridge's own copy, `prefs.rs`) and goes
//! back the same way; a slider says its value in the unit it is in.

use dioxus::prelude::*;

use crate::ui::widgets::{Seg, Slider, SwitchRow};

/// The units a slider may be in. Speed is in `Times`; the rest are for the knobs the touchpad
/// research is adding (a tap threshold in px, delays in ms, sensitivities in %).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unit {
    /// `1.25×`
    Times,
    /// `12%` of 1.0
    Percent,
    /// `24 ms`
    Ms,
    /// `8 px`
    Px,
}

impl Unit {
    pub fn show(self, v: f64) -> String {
        match self {
            Unit::Times => format!("{v:.2}×"),
            Unit::Percent => format!("{}%", (v * 100.0).round()),
            Unit::Ms => format!("{} ms", v.round()),
            Unit::Px => format!("{} px", v.round()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Switch,
    Slider {
        min: f64,
        max: f64,
        step: f64,
        unit: Unit,
    },
    /// `(value as JSON, label)`: `("\"keys\"", "Keys")`, `("true", "Sideways")`.
    Choice(&'static [(&'static str, &'static str)]),
    /// The same, drawn as pictures of themselves: `(value, label, picture class)`.
    Pictures(&'static [(&'static str, &'static str, &'static str)]),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spec {
    /// The field in the device's block (`device.<key>.trackpad.<key>`).
    pub key: &'static str,
    pub title: &'static str,
    pub sub: &'static str,
    pub kind: Kind,
    /// The preview's demo to run when it changes (`js/input_preview/`).
    pub demo: &'static str,
    /// Only while this boolean field is on.
    pub needs: Option<&'static str>,
    /// Shown only when the computer says it can apply it (`ConfigState.caps`): `pad-hypr` for
    /// what Hyprland sets per device; a feature still to come names a cap no daemon sends yet.
    pub cap: Option<&'static str>,
}

/// The value of `key` in a JSON model, as a number / bool / the JSON literal choices compare by.
pub fn num(model: &serde_json::Value, key: &str, dflt: f64) -> f64 {
    model.get(key).and_then(|v| v.as_f64()).unwrap_or(dflt)
}
pub fn flag(model: &serde_json::Value, key: &str) -> bool {
    model.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}
fn literal(model: &serde_json::Value, key: &str) -> String {
    model.get(key).map(|v| v.to_string()).unwrap_or_default()
}

#[component]
pub fn Setting(
    spec: Spec,
    model: serde_json::Value,
    onset: EventHandler<(Spec, serde_json::Value)>,
) -> Element {
    let disabled = spec.needs.is_some_and(|k| !flag(&model, k));
    let parse =
        |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap_or(serde_json::Value::Null);
    match spec.kind {
        Kind::Switch => rsx! {
            SwitchRow {
                title: spec.title,
                sub: spec.sub,
                on: flag(&model, spec.key),
                disabled,
                ontoggle: move |v: bool| onset.call((spec, serde_json::Value::Bool(v))),
            }
        },
        Kind::Slider {
            min,
            max,
            step,
            unit,
        } => {
            let v = num(&model, spec.key, (min + max) / 2.0);
            rsx! {
                div { class: "visetting",
                    Slider {
                        label: spec.title,
                        value: v,
                        min, max, step,
                        shown: unit.show(v),
                        disabled,
                        oninput: move |x: f64| onset.call((spec, serde_json::json!((x * 1000.0).round() / 1000.0))),
                    }
                    if !spec.sub.is_empty() { small { class: "vicap", "{spec.sub}" } }
                }
            }
        }
        Kind::Choice(items) => {
            let cur = literal(&model, spec.key);
            rsx! {
                div { class: "visetting",
                    if !spec.title.is_empty() { div { class: "cardhead", "{spec.title}" } }
                    Seg {
                        options: items.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect(),
                        value: cur,
                        disabled,
                        onpick: move |v: String| onset.call((spec, parse(&v))),
                    }
                    if !spec.sub.is_empty() { small { class: "vicap", "{spec.sub}" } }
                }
            }
        }
        Kind::Pictures(items) => {
            let cur = literal(&model, spec.key);
            rsx! {
                div { class: "vichoice2", role: "radiogroup", "aria-label": "{spec.title}",
                    for (v, label, pic) in items.iter().copied() {
                        button {
                            key: "{v}",
                            class: "vichoice",
                            role: "radio",
                            "aria-checked": "{cur == v}",
                            disabled,
                            onclick: move |_| onset.call((spec, parse(v))),
                            span { class: "viph {pic}", "aria-hidden": "true" }
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
