//! A row that opens a page: tinted icon disc, title, a summary of what is set, chevron.

use dioxus::prelude::*;

use super::Icon;

#[component]
pub fn NavRow(
    icon: String,
    /// base16 slot for the disc, e.g. `"0D"`.
    hue: String,
    title: String,
    sub: String,
    onopen: EventHandler<()>,
) -> Element {
    rsx! {
        button { class: "navrow", onclick: move |_| onopen.call(()),
            span { class: "disc", style: "--hue:var(--base{hue})", Icon { name: icon } }
            span { class: "navtext", b { "{title}" } small { "{sub}" } }
            Icon { name: "right" }
        }
    }
}
