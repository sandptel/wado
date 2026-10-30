//! The Android app's two asks of this client (`js/native.js`; the app is `android/`). In a
//! browser neither ever arrives.
//!
//! - **Go**: the app's own home (or its share sheet) picked a computer, and maybe a door on it —
//!   the same doors as the app-icon shortcuts ([`crate::deeplink`]), followed the same way.
//! - **Back**: the system back gesture, once the page has no history of its own left. It steps
//!   back one view; from home it tells the app there is nowhere left, and the app shows its own.

use dioxus::prelude::*;

use crate::{
    bridge,
    deeplink::Go,
    route::{self, View},
    state::Ui,
};

/// Open the computer with this profile key, then the door `go` names (empty: just its page).
/// Nothing happens for a computer this client does not have, or during a session — a session
/// is where the person is.
pub fn go(ui: Ui, key: &str, go: &str) {
    if route::index_of(ui, key).is_none() || (ui.live.session_on)() {
        return;
    }
    route::open_computer(ui, key);
    if let Some(g) = Go::parse(go) {
        let mut live = ui.live;
        live.go.set(Some(g));
    }
}

/// One step back: a page to its computer, a computer to home (with its exit motion), home to the
/// app. In a session, Back opens or closes the control centre instead of leaving the picture.
pub fn back(ui: Ui) {
    let mut live = ui.live;
    if (live.session_on)() {
        let open = (live.cc_open)();
        live.cc_open.set(!open);
        return;
    }
    let view = live.view.peek().clone();
    match view {
        View::Home => bridge::call("window.__wado.nativeExit();".to_string()),
        View::Computer(key) => {
            bridge::call(format!("window.__wado.heroClose({});", bridge::js(&key)))
        }
        View::Sub(..) => route::back(ui),
    }
}
