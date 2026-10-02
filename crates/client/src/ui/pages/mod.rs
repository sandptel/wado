//! The control centre's drill-in pages, one file each, grouped by subject.
//!
//! When a setting applies is said on the control (see [`crate::ui::widgets::When`]), not by
//! which page it is on — the page answers "what is this about", the badge answers "when".

pub mod appearance;
pub mod connection;
pub mod diagnostics;
pub mod display;
pub mod gamepad;
pub mod host;
pub mod input;
pub mod network;
pub mod sound;
pub mod switcher;
pub mod tiles;

use dioxus::prelude::*;

use crate::state::Ui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Root,
    Display,
    Input,
    Switcher,
    Gamepad,
    Appearance,
    Connection,
    Diagnostics,
    Host,
    Tiles,
    Sound,
    Network,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Root => "Control centre",
            Page::Display => "Display & stream",
            Page::Input => "Input & touch",
            Page::Switcher => "Window switcher",
            Page::Gamepad => "Gamepad",
            Page::Appearance => "Appearance",
            Page::Connection => "Connection",
            Page::Diagnostics => "Diagnostics",
            Page::Host => "This computer",
            Page::Tiles => "Edit tiles",
            Page::Sound => "Sound",
            Page::Network => "Wi-Fi & Bluetooth",
        }
    }
}

pub fn render(ui: Ui, page: Page) -> Element {
    match page {
        Page::Root => rsx! {},
        Page::Display => display::render(ui),
        Page::Input => input::render(ui),
        Page::Switcher => switcher::render(ui),
        Page::Gamepad => gamepad::render(ui),
        Page::Appearance => appearance::render(ui),
        Page::Connection => connection::render(ui),
        Page::Diagnostics => diagnostics::render(ui),
        Page::Host => host::render(ui),
        Page::Tiles => tiles::render(ui),
        Page::Sound => sound::render(ui),
        Page::Network => network::render(ui),
    }
}

/// `(value, label)` pairs from string slices — what [`crate::ui::widgets::Seg`] takes.
pub fn opts(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect()
}
