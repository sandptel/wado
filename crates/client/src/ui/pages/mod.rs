//! Settings pages: which there are, and the sections they are built from.
//!
//! Since the Computer page (2026-10-30) there are two kinds, and every page says which:
//!
//! | about | pages | built in |
//! |---|---|---|
//! | one computer | Display & session, Sound & devices, Connection & trust | `ui/computer/pages/` |
//! | this phone | Appearance, Input feel, Gamepad, Quick tiles, Diagnostics, Add a computer | `ui/home/pages/` |
//!
//! Each page is one component with two hosts: pushed over home or a computer's page, and pushed
//! inside the control centre during a session (whose list is just *This computer ›* and *On this
//! phone ›*). The files in this folder are the **sections** those pages are made of.
//!
//! A computer's **Trackpad** settings are not a page here: its row opens the trackpad's own
//! settings sheet (`js/pad_settings.js`, `W.padSettings.openHere`) — one implementation, reached
//! from the pad and from the computer's page.
//!
//! When a setting applies is said on the control (see [`crate::ui::widgets::When`]), not by
//! which page it is on — the page answers "what is this about", the badge answers "when".

pub mod about;
pub mod appearance;
pub mod computers;
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
    /// The control centre's own root.
    #[default]
    Root,
    /// The control centre's two lists.
    ThisComputer,
    OnThisPhone,
    // ── about one computer ──
    DisplaySession,
    SoundDevices,
    ConnectionTrust,
    // ── about this phone ──
    Appearance,
    InputFeel,
    Gamepad,
    Tiles,
    Diagnostics,
    AddComputer,
}

impl Page {
    pub fn title(self) -> &'static str {
        match self {
            Page::Root => "Control centre",
            Page::ThisComputer => "This computer",
            Page::OnThisPhone => "On this phone",
            Page::DisplaySession => "Display & session",
            Page::SoundDevices => "Sound & devices",
            Page::ConnectionTrust => "Connection & trust",
            Page::Appearance => "Appearance",
            Page::InputFeel => "Input feel",
            Page::Gamepad => "Gamepad",
            Page::Tiles => "Quick tiles",
            Page::Diagnostics => "Diagnostics",
            Page::AddComputer => "Add a computer",
        }
    }

    /// The page changes one computer — its head names which.
    pub fn about_computer(self) -> bool {
        matches!(
            self,
            Page::ThisComputer | Page::DisplaySession | Page::SoundDevices | Page::ConnectionTrust
        )
    }

    /// Back, inside the control centre.
    pub fn parent(self) -> Page {
        match self {
            Page::Root | Page::ThisComputer | Page::OnThisPhone => Page::Root,
            p if p.about_computer() => Page::ThisComputer,
            _ => Page::OnThisPhone,
        }
    }
}

pub fn render(ui: Ui, page: Page) -> Element {
    use crate::ui::{computer::pages as c, home::pages as h};
    match page {
        Page::Root => rsx! {},
        Page::ThisComputer => crate::ui::computer::settings_list::render(ui),
        Page::OnThisPhone => crate::ui::home::settings::render(ui),
        Page::DisplaySession => {
            crate::ui::scoped(ui, "display_session", c::display_session::render)
        }
        Page::SoundDevices => crate::ui::scoped(ui, "sound_devices", c::sound_devices::render),
        Page::ConnectionTrust => {
            crate::ui::scoped(ui, "connection_trust", c::connection_trust::render)
        }
        Page::Appearance => crate::ui::scoped(ui, "appearance", h::appearance::render),
        Page::InputFeel => input::feel(ui),
        Page::Gamepad => gamepad::render(ui),
        Page::Tiles => tiles::render(ui),
        Page::Diagnostics => diagnostics::render(ui),
        Page::AddComputer => crate::ui::scoped(ui, "add_computer", h::add_computer::render),
    }
}

/// `(value, label)` pairs from string slices — what [`crate::ui::widgets::Seg`] takes.
pub fn opts(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect()
}
