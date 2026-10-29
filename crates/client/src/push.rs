//! Web Push for this device from the selected computer, as the UI holds it (`js/push.js`).

use dioxus::prelude::*;
use wado_protocol::push::PushInfo;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PushUi {
    /// What the computer said: its key, whether this device is subscribed, which kinds.
    pub info: Option<PushInfo>,
    /// The browser can do Web Push at all (service worker, PushManager, a secure page).
    pub support: bool,
    /// Opened as the installed app — iOS delivers pushes only then.
    pub standalone: bool,
    /// `default` | `granted` | `denied` | `unsupported`.
    pub permission: String,
    /// The Remote ID this browser's one subscription belongs to, and the selected computer's.
    pub from: String,
    pub here: String,
    pub busy: bool,
    pub error: String,
}

pub static PUSH: GlobalSignal<PushUi> = Signal::global(PushUi::default);

pub fn on_event(msg: &serde_json::Value) {
    let s = |k: &str| {
        msg.get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let b = |k: &str| msg.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    *PUSH.write() = PushUi {
        info: msg
            .get("info")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        support: b("support"),
        standalone: b("standalone"),
        permission: s("permission"),
        from: s("from"),
        here: s("here"),
        busy: b("busy"),
        error: s("error"),
    };
}
