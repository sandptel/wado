//! The client's UI, one job per module.
//!
//! | | |
//! |---|---|
//! | [`landing`] | before a session: hosts, connection progress, Start |
//! | [`cc`] | the control centre: hero, quick tiles, settings list, Apply |
//! | [`pages`] | the control centre's drill-in pages, one per subject |
//! | [`dock`] | ⋯ ○ ◁ over the picture |
//! | [`stage`] | the video and everything drawn on it |
//! | [`console`], [`drawer`] | the shell and the app drawer sheets |
//! | [`widgets`] | the controls the pages are built from |
//!
//! [`live`] and [`gamepad`] hold no UI any more: they push browser-side settings to the bridge.

pub mod cc;
pub mod console;
pub mod dock;
pub mod drawer;
pub mod fetch;
pub mod gamepad;
pub mod gesture;
pub mod health;
pub mod host;
pub mod landing;
pub mod live;
pub mod media;
pub mod pages;
pub mod rejoin;
pub mod stage;
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
/// watchdog's report. Keyed by the function, so a different page is a fresh scope.
pub fn scoped(ui: Ui, f: fn(Ui) -> Element) -> Element {
    let key = format!("{:p}", f as *const ());
    rsx! { Scoped { key: "{key}", ui, f } }
}

#[component]
fn Scoped(ui: Ui, f: fn(Ui) -> Element) -> Element {
    f(ui)
}
