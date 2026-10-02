//! A segmented control: a row of equal-width choices with a knob that slides to the active one.
//!
//! Equal widths are what let the knob be pure CSS — `--i` and `--n` place it — so moving it
//! costs no measurement and no script.

use dioxus::prelude::*;

#[component]
pub fn Seg(
    /// `(value, label)` pairs.
    options: Vec<(String, String)>,
    value: String,
    #[props(default)] disabled: bool,
    onpick: EventHandler<String>,
) -> Element {
    let i = options.iter().position(|(v, _)| *v == value);
    let style = format!("--n:{};--i:{}", options.len().max(1), i.unwrap_or(0));
    rsx! {
        div { class: if i.is_some() { "seg" } else { "seg none" }, style: "{style}", role: "radiogroup",
            span { class: "knob" }
            for (v, label) in options.into_iter() {
                button {
                    key: "{v}",
                    class: if v == value { "on" } else { "" },
                    role: "radio",
                    "aria-checked": "{v == value}",
                    disabled,
                    onclick: move |_| onpick.call(v.clone()),
                    "{label}"
                }
            }
        }
    }
}
