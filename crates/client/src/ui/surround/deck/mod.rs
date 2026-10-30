//! The deck: what the active input face puts under the thumbs (spec §3). A band under the chin in
//! portrait, a column beside the picture in landscape and on a desktop.
//!
//! Every face drives **the session**, over the input channel it already has. None of them goes
//! through the trackpad's own link (`js/pad_link.js`): that drives the computer's *own* desktop
//! through a virtual touchpad, which is a different screen from the one in this picture.
//!
//! Only the active face is rendered, keyed by face, so a change re-mounts it and it rises in
//! (`surround.css`, S8). Nothing here re-renders on input: the faces' live state is the page's.

pub mod gamepad;
pub mod keys;
pub mod pad;
pub mod quick;

use dioxus::prelude::*;

use super::layout::Face;
use crate::state::Ui;

pub fn render(ui: Ui, face: Face) -> Element {
    rsx! {
        div { class: "cdeck", role: "group", "aria-label": "Input: {face.label()}", "data-face": "{face.id()}",
            div { key: "{face.id()}", class: "cface",
                match face {
                    Face::Pad => pad::render(),
                    Face::Keys => keys::render(),
                    Face::Quick => crate::ui::scoped(ui, "deck-quick", quick::render),
                    Face::Gamepad => gamepad::render(),
                }
            }
        }
    }
}
