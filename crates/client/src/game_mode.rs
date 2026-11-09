//! Game mode, on the web — the same rules as the app (`wado_ux::game`, shared): a switch on one app
//! of one computer, set up once. While a session runs for it, its overlay shapes the session config
//! ([`shape`]) and shadows how the console opens, its face, the pad over the picture and the pad's
//! layout (`js/game_shadow.js`) — never written over this device's own settings or its prefs on the
//! computer. A change made in the game is kept for the game ([`kept`]).

use std::collections::BTreeMap;

use dioxus::prelude::*;
use serde_json::Value;
use wado_protocol::{Quality, SessionConfig};
pub use wado_ux::game::{needs_x, new, picture_words, play_words, Profile};

use crate::{bridge, state::Ui};

/// The screen's refresh, as the app reads it; the web has no API for it, so a common 120 cap
/// stands in and the computer clamps to what it can encode.
// ponytail: a constant; `requestAnimationFrame` timing could measure it if 120 reads wrong.
const REFRESH: u32 = 120;

/// The selected computer, as its live status is filed (`profile::Profile::key`).
pub fn computer(ui: Ui) -> String {
    format!("{}|{}", (ui.set.relay_url)(), (ui.set.remote_id)())
}

fn all(ui: Ui) -> BTreeMap<String, Profile> {
    serde_json::from_str(&(ui.set.game_modes)()).unwrap_or_default()
}

fn store(ui: Ui, m: &BTreeMap<String, Profile>) {
    let mut s = ui.set.game_modes;
    s.set(serde_json::to_string(m).unwrap_or_default());
}

/// Every app of `computer` with Game mode on.
pub fn on_for(ui: Ui, computer: &str) -> Vec<Profile> {
    let c = format!("{computer}#");
    all(ui)
        .into_iter()
        .filter(|(k, _)| k.starts_with(&c))
        .map(|(_, p)| p)
        .collect()
}

/// Every app of the selected computer with Game mode on.
pub fn on(ui: Ui) -> Vec<Profile> {
    on_for(ui, &computer(ui))
}

pub fn save(ui: Ui, p: &Profile) {
    let mut m = all(ui);
    m.insert(format!("{}#{}", computer(ui), p.exec), p.clone());
    store(ui, &m);
}

pub fn off(ui: Ui, exec: &str) {
    let mut m = all(ui);
    m.remove(&format!("{}#{exec}", computer(ui)));
    store(ui, &m);
}

/// The running game's rules over the session config, from its overlay's shared names.
pub fn shape(c: &mut SessionConfig, p: &Profile) {
    let o = p.overlay(REFRESH);
    let b = |k: &str| o.get(k).and_then(Value::as_bool);
    if let Some(f) = o.get("fps").and_then(Value::as_u64) {
        c.fps = f as u32;
    }
    c.quality = match o.get("quality").and_then(Value::as_str) {
        Some("reactivity") => Quality::Reactivity,
        Some("quality") => Quality::Quality,
        Some("balanced") => Quality::Balanced,
        _ => c.quality.clone(),
    };
    if let Some(k) = o.get("keyframe_mode").and_then(Value::as_str) {
        c.keyframe_interval = (k == "on_request").then_some(c.fps.max(1) * 10);
    }
    if b("low_latency_audio") == Some(true) {
        c.low_latency_audio = true;
    }
    if b("x_server") == Some(true) {
        c.x_server = true;
    }
}

/// The console choices the game shadows, for `js/game_shadow.js`.
fn shadow_js(p: &Profile) -> String {
    let o = p.overlay(REFRESH);
    let s = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let face = match s("console_face").as_str() {
        "touchpad" => "pad".to_string(),
        "" => "auto".to_string(),
        f => f.to_string(),
    };
    let v = serde_json::json!({
        "open": if s("console_open") == "full" { "full" } else { "console" },
        "face": face,
        "pad": o.get("pad_on").and_then(Value::as_bool).unwrap_or(false),
        "layout": o.get("gpad_layout").and_then(Value::as_str),
    });
    format!("window.__wado.gameShadow({v});")
}

/// Open `p`'s app as a game: a session shaped by its rules, its console choices shadowed, and the
/// app launched once it is up (`ui::home::games` launcher) — or straight into the running session.
pub fn open(ui: Ui, p: Profile) {
    if crate::host_caps::refused(ui, "session") {
        return;
    }
    let mut live = ui.live;
    crate::recent::note(ui, "game", &p.name, &p.exec);
    bridge::call(shadow_js(&p));
    live.game.set(Some(p.clone()));
    if (live.session_on)() {
        crate::actions::launch_command(ui, &p.exec);
    } else {
        live.game_launch.set(Some(p.exec.clone()));
        crate::actions::start(ui);
    }
}

/// The session ended: the device's own console choices and pad switch back.
pub fn end(ui: Ui) {
    let mut live = ui.live;
    if live.game.peek().is_none() {
        return;
    }
    live.game.set(None);
    bridge::call("window.__wado.gameShadow(null);".to_string());
    crate::ui::gamepad::apply(ui);
}

/// A shadowed choice changed in the game (`gameKept`): kept for the game.
pub fn kept(ui: Ui, key: &str, value: Value) {
    let mut live = ui.live;
    let Some(mut p) = live.game.peek().clone() else {
        return;
    };
    p.keep(key, value);
    save(ui, &p);
    live.game.set(Some(p));
}
