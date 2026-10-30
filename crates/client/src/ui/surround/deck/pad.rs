//! The Touchpad face: a surface that moves the session's pointer by how far a finger slides, and
//! a two-zone click bar. `js/deck_pad.js` owns every pointer event on it (by delegation, so this
//! markup can re-mount freely) and draws the pointer it moves over the picture.

use dioxus::prelude::*;

pub fn render() -> Element {
    rsx! {
        div { id: "deck-pad", class: "dpsurf", role: "application",
            "aria-label": "Touchpad: slide to move the pointer, tap to click, two fingers to scroll",
            span { class: "dphint", "aria-hidden": "true",
                b { "Touchpad" }
                small { "Slide to move · tap to click · two fingers to scroll" }
            }
        }
        div { class: "dpbtns",
            button { class: "dpbtn", "data-dbtn": "left", "aria-label": "Left click", "Left" }
            button { class: "dpbtn", "data-dbtn": "right", "aria-label": "Right click", "Right" }
        }
    }
}
