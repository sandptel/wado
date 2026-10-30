//! Appearance: how big this phone draws wado (text size, interface size), then the scheme,
//! accent, corners and motion — and the dock and switcher, which are this phone's chrome around
//! any computer's desktop, with how a session opens (the Pocket Console or full screen) and the
//! console's input face.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::pages::{appearance, arrangement, size::SizeCards, switcher},
};

pub fn render(ui: Ui) -> Element {
    rsx! {
        SizeCards {}
        div { class: "sect", span { "Colour & shape" } }
        {appearance::render(ui)}
        div { class: "sect", span { "Dock & switcher" } }
        {arrangement::render(ui)}
        {switcher::render(ui)}
    }
}
