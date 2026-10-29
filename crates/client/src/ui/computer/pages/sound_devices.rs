//! Sound & devices: the computer's mixer — everything playing and every output — and its
//! Bluetooth devices. Works with no session: only the relay link is needed.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::pages::{network, sound},
};

pub fn render(ui: Ui) -> Element {
    rsx! {
        {sound::render(ui)}
        div { class: "sect", span { "Bluetooth" } }
        {network::bluetooth(ui)}
    }
}
