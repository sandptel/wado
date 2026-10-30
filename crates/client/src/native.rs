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
/// Nothing happens for a computer this client does not have.
///
/// During a session, a door is a choice to be somewhere else: this device leaves the session
/// (it keeps running on the computer) and goes there. Refusing it — "a session is where the
/// person is" — was the trap of 2026-10-30: every door in the app's own home led back into a
/// session that had no picture and no way out. Only the same computer's desktop stays put.
pub fn go(ui: Ui, key: &str, go: &str) {
    if route::index_of(ui, key).is_none() {
        return;
    }
    if (ui.live.session_on)() {
        let same = crate::profile::current_key(ui) == key;
        if same && Go::parse(go) == Some(Go::Session) {
            return;
        }
        crate::actions::leave(ui);
    }
    route::open_computer(ui, key);
    if let Some(g) = Go::parse(go) {
        let mut live = ui.live;
        live.go.set(Some(g));
    }
}

/// One step back: a page to its computer, a computer to home (with its exit motion), home to the
/// app. In a session, Back closes the control centre if it is up, and otherwise leaves the
/// session for the computer's page — never toggling the control centre open, which made Back a
/// way *into* the session's chrome rather than out of it.
pub fn back(ui: Ui) {
    let mut live = ui.live;
    if (live.session_on)() {
        if (live.cc_open)() {
            live.cc_open.set(false);
        } else {
            crate::actions::leave(ui);
        }
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
