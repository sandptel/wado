//! The passkey step-up, as the UI sees it: where this device stands on the selected computer,
//! and which door — if any — is waiting for an unlock. The ceremony itself is `js/passkey.js`.

use dioxus::prelude::*;
use wado_protocol::passkey::PasskeyState;

use crate::{bridge, state::Ui};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepUp {
    pub state: PasskeyState,
    /// A ceremony is underway (the phone's passkey prompt is up, or the computer is checking).
    pub busy: bool,
    /// The door asking: `shell`, `pad`, `files`, `allow`, `claude`, `new`, `add`, `paired`,
    /// `again`. Empty: no sheet.
    pub need: String,
    /// Why the last try failed, in plain words.
    pub note: String,
    /// This browser has WebAuthn at all, and a screen-lock authenticator built in.
    pub supported: bool,
    pub platform: bool,
    /// This page's origin, to tell a passkey made on another copy of the app.
    pub origin: String,
    /// Bumped on every unlock, so the lock glyph can play its opening once.
    pub opened: u32,
}

pub static STEP: GlobalSignal<StepUp> = Signal::global(StepUp::default);

/// `{type:"passkey", …}` from the bridge.
pub fn on_event(ui: Ui, msg: &serde_json::Value) {
    let get = |k: &str| msg.get(k);
    let s = |k: &str| get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let b = |k: &str| get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let state: PasskeyState = get("state")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let was = STEP.peek().state.unlocked;
    let opened = STEP.peek().opened + u32::from(state.unlocked && !was);
    *STEP.write() = StepUp {
        state,
        busy: b("busy"),
        need: s("need"),
        note: s("note"),
        supported: b("supported"),
        platform: b("platform"),
        origin: s("origin"),
        opened,
    };
    // The shell door is Rust's own; the others carry their action in `js/passkey.js`.
    if s("go") == "shell" {
        crate::ui::console::open_shells_now(ui);
    }
}

/// Open `door` behind the passkey: at once when unlocked, else after the sheet. `js` runs then,
/// or — when empty — Rust's own action for `door` (see [`on_event`]).
pub fn then(door: &str, js: &str) {
    let f = if js.is_empty() {
        "null".to_string()
    } else {
        format!("() => {{ {js} }}")
    };
    bridge::call(format!(
        "window.__wado.passkeyThen({}, {f});",
        bridge::js(&door)
    ));
}
