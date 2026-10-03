//! Session lifecycle actions: what the Start / Stop / Launch buttons do.
//!
//! Separate from the widgets that trigger them because the ordering matters and is easy to get
//! subtly wrong — optimistic local state first so the UI responds immediately, then the eval.

use dioxus::prelude::*;

use crate::{bridge, cfg, state::Ui};

pub fn start(ui: Ui) {
    let mut live = ui.live;
    let s = ui.set;
    let config = cfg::build(ui);

    // Relay opts are null in direct mode, which the bridge treats as falsy.
    let relay = ((s.conn_mode)() == "relay")
        .then(|| serde_json::json!({ "relayUrl": (s.relay_url)(), "remoteId": (s.remote_id)() }));

    live.session_on.set(true);
    live.clear_telemetry();
    // A new session starts at the full rate; auto bitrate learns this link afresh.
    live.auto_kbps.set(None);
    bridge::call("window.__wado.autorate.reset();".to_string());
    live.status.set("starting session…".to_string());
    live.applied
        .set(serde_json::to_string(&config).unwrap_or_default());
    crate::profile::remember(ui);
    // Start is a gesture: the one moment a phone lets audio begin without another tap.
    bridge::call("window.__wado.audioUnlock();".to_string());

    bridge::call(format!(
        "window.__wado.start({}, {}, {});",
        bridge::js(&(s.server_addr)()),
        bridge::js(&config),
        bridge::js(&relay),
    ));
}

/// Launch one specific command — what a drawer tile does, where there is no box to read.
pub fn launch_command(ui: Ui, command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    remember(ui, command);
    bridge::call(format!("window.__wado.launch({});", bridge::js(&command)));
}

/// Move `command` to the front of the recents list.
///
/// Deduplicated, so launching the same thing twice does not fill the row with one app; capped,
/// because the row is one line on a phone.
fn remember(ui: Ui, command: &str) {
    let mut recent = ui.set.recent;
    let mut list = recent.read().clone();
    list.retain(|c| c != command);
    list.insert(0, command.to_string());
    list.truncate(crate::state::MAX_RECENT);
    recent.set(list);
}

/// Apply the current settings to the session that is **already running**.
///
/// The difference from `start` that matters: nothing is torn down. Before this existed,
/// changing a bitrate or a resolution meant Stop then Start, and Stop kills every application
/// the session launched — so the cost of turning one knob was the browser and everything in it.
pub fn apply(ui: Ui) {
    let mut live = ui.live;
    let config = cfg::build(ui);
    live.status.set("applying…".to_string());
    live.applied
        .set(serde_json::to_string(&config).unwrap_or_default());
    bridge::call(format!(
        "window.__wado.reconfigure({});",
        bridge::js(&config)
    ));
}

/// Turn the session a quarter: `to` is `landscape` or `portrait`. The output is re-made at the
/// swapped size (invariant #8) and the windows re-fit, as a phone does when it is turned.
pub fn rotate(ui: Ui, to: &str) {
    let mut s = ui.set;
    let c = cfg::build(ui);
    let (w, h) = (c.width.max(c.height), c.width.min(c.height));
    let (w, h) = if to == "landscape" { (w, h) } else { (h, w) };
    s.custom_w.set(w);
    s.custom_h.set(h);
    s.res.set("custom".to_string());
    s.orientation.set(to.to_string());
    crate::ui::live::apply(ui);
    if (ui.live.session_on)() {
        apply(ui);
    }
}

/// Go home and leave the session running — kept by the daemon until someone ends it.
pub fn leave(ui: Ui) {
    let mut live = ui.live;
    live.session_on.set(false);
    live.clear_telemetry();
    live.applied.set(String::new());
    live.cc_open.set(false);
    live.status.set("left the session running".to_string());
    bridge::call("window.__wado.leaveSession();".to_string());
}

pub fn stop(ui: Ui) {
    let mut live = ui.live;
    live.session_on.set(false);
    live.clear_telemetry();
    live.applied.set(String::new());
    live.status.set("idle".to_string());
    live.stagebar.set("No session.".to_string());
    bridge::call("window.__wado.stopSession();".to_string());
}
