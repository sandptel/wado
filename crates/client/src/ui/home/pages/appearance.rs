//! Appearance: the scheme, accent, corners and motion — and the dock and switcher, which are
//! this phone's chrome around any computer's desktop.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::pages::{appearance, switcher},
};

pub fn render(ui: Ui) -> Element {
    rsx! {
        {appearance::render(ui)}
        div { class: "sect", span { "Dock & switcher" } }
        {switcher::render(ui)}
    }
}
