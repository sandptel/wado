//! The Gamepad face — a thin seam, deliberately.
//!
//! The controls are `js/gamepad.js`'s own, unchanged. While this face is up in the console, the
//! pad's box is the layout's `pad` slot (`layout.rs`; on the page `W.stageFit.slot("pad")` and the
//! `--c-pad-*` variables), and `surround.css` re-anchors the existing clusters for a band deck or
//! a column deck. That is the whole contract: a box to lay out in, and whether it is a band
//! (`data-cdeck="row"`) or a column (`"col"`). A revamped gamepad plugs its own layouts in by
//! reading that slot, and the cluster overrides at the end of `surround.css` go away with it.
//! Nothing gamepad-specific is configured here.

use dioxus::prelude::*;

pub fn render() -> Element {
    rsx! {
        p { class: "dgp", "aria-hidden": "true", "Gamepad" }
    }
}
