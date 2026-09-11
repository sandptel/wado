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
    live.status.set("starting session…".to_string());

    bridge::call(format!(
        "window.__wado.start({}, {}, {});",
        bridge::js(&(s.server_addr)()),
        bridge::js(&config),
        bridge::js(&relay),
    ));
}

pub fn launch(ui: Ui) {
    let command = (ui.set.command)();
    if command.trim().is_empty() {
        return;
    }
    bridge::call(format!("window.__wado.launch({});", bridge::js(&command)));
}

pub fn stop(ui: Ui) {
    let mut live = ui.live;
    live.session_on.set(false);
    live.clear_telemetry();
    live.status.set("idle".to_string());
    live.stagebar.set("No session.".to_string());
    bridge::call("window.__wado.stopSession();".to_string());
}
