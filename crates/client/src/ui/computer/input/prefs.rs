//! This phone's input settings as the JS bridge holds them — the gamepad's (`js/gpad/prefs.js`),
//! the keyboard's (`js/kb_prefs.js`) and the touchpad's (`js/pad_prefs.js`). The bridge is the
//! owner (it keeps the localStorage mirror and writes `device.<key>.…` to the computer); it
//! reports every change here so the pages read the same values the pad uses.

use dioxus::prelude::*;

use crate::bridge;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gamepad {
    pub drives: String,
    pub layout: String,
    pub away: String,
    pub haptics: String,
    pub hand: String,
    /// The stick type and dead zone of the layout used away from a session.
    pub stick: String,
    pub dead: f64,
    /// `(id, name)` of every layout there is: the built-ins, then this phone's own.
    pub list: Vec<(String, String)>,
}

impl Gamepad {
    pub fn name_of(&self, id: &str) -> String {
        self.list
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "Standard".into())
    }
    /// The layout away from a session, as words.
    pub fn away_name(&self) -> String {
        if self.away == "same" || self.away.is_empty() {
            self.name_of(&self.layout)
        } else {
            self.name_of(&self.away)
        }
    }
}

pub static GAMEPAD: GlobalSignal<Gamepad> = Signal::global(Gamepad::default);
pub static KEYS: GlobalSignal<Vec<String>> = Signal::global(Vec::new);
/// The trackpad's prefs as the bridge holds them, whole (`W.padPrefs.get()`): one JSON model
/// the settings table reads by key (`setting.rs`), so a new field needs no struct here.
pub static TOUCHPAD: GlobalSignal<serde_json::Value> = Signal::global(|| serde_json::Value::Null);

/// `{type:"gpadPrefs" | "kbPrefs" | "padPrefs", …}` from the bridge.
pub fn on_event(kind: &str, msg: &serde_json::Value) {
    let s = |v: &serde_json::Value, k: &str| {
        v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
    };
    match kind {
        "gpadPrefs" => {
            let p = msg.get("prefs").cloned().unwrap_or_default();
            let list = msg
                .get("list")
                .and_then(|l| l.as_array())
                .map(|a| a.iter().map(|it| (s(it, "id"), s(it, "name"))).collect())
                .unwrap_or_default();
            *GAMEPAD.write() = Gamepad {
                drives: s(&p, "drives"),
                layout: s(&p, "layout"),
                away: s(&p, "away"),
                haptics: s(&p, "haptics"),
                hand: s(&p, "hand"),
                stick: s(msg, "stick"),
                dead: msg.get("dead").and_then(|x| x.as_f64()).unwrap_or(0.12),
                list,
            };
        }
        "kbPrefs" => {
            *KEYS.write() = msg
                .get("keys")
                .and_then(|l| l.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|k| k.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
        }
        "padPrefs" => *TOUCHPAD.write() = msg.get("prefs").cloned().unwrap_or_default(),
        _ => {}
    }
}

/// Set one gamepad field through the bridge (`W.gpadPrefs.set`).
pub fn set_gamepad(field: &str, value: &str) {
    bridge::call(format!(
        "window.__wado.gpadPrefs.set({}, {});",
        bridge::js(&field),
        bridge::js(&value)
    ));
}

/// Set one touchpad field through the bridge (`W.padPrefs.set`).
pub fn set_touchpad(field: &str, value: serde_json::Value) {
    bridge::call(format!(
        "window.__wado.padPrefs.set({}, {});",
        bridge::js(&field),
        value
    ));
}

/// Run a demo in a settings preview (`js/input_preview/`).
pub fn demo(which: &str, name: &str) {
    bridge::call(format!(
        "window.__wado.inputPreview && window.__wado.inputPreview.demo({}, {});",
        bridge::js(&which),
        bridge::js(&name)
    ));
}
