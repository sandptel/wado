//! The lock, drawn as two strokes — shackle and body — so the shackle can lift and swing open on
//! an unlock (`.pkhead.open` in `passkey.css`) while the body pops. Decorative: the heading says
//! it in words.

use dioxus::prelude::*;

#[component]
pub fn Lock() -> Element {
    rsx! {
        svg {
            class: "pklock",
            view_box: "0 0 48 48",
            "aria-hidden": "true",
            path { class: "shackle", d: "M15 21v-6a9 9 0 0 1 18 0v6" }
            rect { class: "body", x: "9", y: "21", width: "30", height: "21", rx: "6" }
            circle { class: "keyhole", cx: "24", cy: "31", r: "2.6" }
        }
    }
}
