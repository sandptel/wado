//! The client's UI, one job per module.
//!
//! | | |
//! |---|---|
//! | [`landing`] | home's frame: before a session, the lobby of computers |
//! | [`home`] | home's parts: computer cards, Running, On this phone, widgets |
//! | [`computer`] | one computer's page, and the pages about it |
//! | [`subpage`] | a settings page pushed over home or a computer's page |
//! | [`cc`] | the control centre: hero, quick tiles, settings list, Apply |
//! | [`clipboard`] | a computer's clipboard surface, and the clipboard's voice over everything |
//! | [`pages`] | the page list, and the sections the pages are built from |
//! | [`dock`] | ⋯ ○ ◁ over the picture |
//! | [`stage`] | the video and everything drawn on it |
//! | [`console`], [`drawer`] | the shell and the app drawer sheets |
//! | [`widgets`] | the controls the pages are built from |
//!
//! [`live`] and [`gamepad`] hold no UI any more: they push browser-side settings to the bridge.

pub mod cc;
pub mod claude;
pub mod clipboard;
pub mod computer;
pub mod console;
pub mod dock;
pub mod drawer;
pub mod fetch;
pub mod gamepad;
pub mod gesture;
pub mod health;
pub mod home;
pub mod host;
pub mod landing;
pub mod latency;
pub mod link_badge;
pub mod live;
pub mod pages;
pub mod passkey;
pub mod push;
pub mod reconnecting;
pub mod rejoin;
pub mod stage;
pub mod subpage;
pub mod toast;
pub mod widgets;
pub mod workspaces;

use dioxus::prelude::*;

use crate::state::Ui;

/// Render `f` in a component scope of its own.
///
/// **Any render function that calls a hook (`use_signal`, `use_effect`) goes through this.**
/// Called as a plain function, its hooks land in the caller's scope, and a caller that only
/// sometimes calls it — a control-centre page, a card shown while something plays — changes the
/// hook order between renders. Dioxus then panics ("Unable to retrieve the hook that was
/// initialized at this index"), the WASM UI is dead, and every button stops while the video
/// plays on: the "all buttons go unresponsive mid-session" of 2026-10-03, caught by the
/// watchdog's report. Keyed by `name`, so a different page is a fresh scope.
pub fn scoped(ui: Ui, name: &'static str, f: fn(Ui) -> Element) -> Element {
    rsx! { Scoped { key: "{name}", ui, f: Render(f) } }
}

/// The render function, as a prop. Its identity is the scope's key, so props never differ by it.
#[derive(Clone, Copy)]
struct Render(fn(Ui) -> Element);

impl PartialEq for Render {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[component]
fn Scoped(ui: Ui, f: Render) -> Element {
    (f.0)(ui)
}
