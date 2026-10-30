//! Virtual input: this phone as the computer's **Touchpad**, **Keyboard** and **Gamepad**, with
//! or without a session (docs/superpowers/specs/2026-10-30-virtual-input-design.md). One tile on
//! the Computer page, a hub, and a settings page for each, each led by a live preview.
//!
//! | file | job |
//! |---|---|
//! | [`open`] | the three doors (and the hub), shared by the tile, the hub, the widget and `?go=` |
//! | [`tile`] | the Computer page's group tile: heading → hub, three one-tap doors |
//! | [`grant_line`] | allowed · unlocked / allowed · locked / not allowed, never hidden |
//! | [`hub`] | the hub: three live device cards, "On all three", the computer's own switches |
//! | [`touchpad`] | Touchpad settings: preview, how you hold it, the rail, pointer speed, gestures |
//! | [`keyboard`] | Keyboard settings: preview, the keys above the keyboard, shortcuts |
//! | [`gamepad`] | Gamepad settings: live pad, layouts, away from a session, drives, sticks |
//! | [`setting`] | one setting from a spec: switch, slider, choice or pictures |
//! | [`prefs`] | this phone's input settings as the JS bridge reports them |
//!
//! The previews, the gamepad and its editor are JS-owned (`js/input_preview/`, `js/gpad/`):
//! Rust renders the boxes they draw into (`data-vi`) and nothing inside them.

pub mod gamepad;
pub mod grant_line;
pub mod hub;
pub mod keyboard;
pub mod open;
pub mod prefs;
pub mod setting;
pub mod tile;
pub mod touchpad;

use dioxus::prelude::*;

use crate::ui::widgets::Icon;

/// A page's head action: the device's own Open / Play, so settings are never a dead end (D5).
#[component]
pub fn HeadOpen(label: String, disabled: bool, onopen: EventHandler<()>) -> Element {
    rsx! {
        button { class: "btn primary viopen", disabled, onclick: move |_| onopen.call(()),
            Icon { name: "play", fill: true }
            "{label}"
        }
    }
}
