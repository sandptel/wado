//! The Gamepad face. The clusters are `js/gamepad.js`'s own, unchanged; while this face is up the
//! pad's box is the layout's `pad` slot (`surround.css`), so the same corner anchors put the d-pad
//! and the face buttons under the thumbs instead of over the picture.

use dioxus::prelude::*;

pub fn render() -> Element {
    rsx! {
        p { class: "dgp", "aria-hidden": "true", "Gamepad" }
    }
}
