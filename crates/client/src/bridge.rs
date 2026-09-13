//! The JS bridge: the script itself, the helpers that poke it, and the pump that drains its
//! events into signals.
//!
//! Everything browser-only lives on the JS side (`js/`), driven through Dioxus `eval`: one
//! long-lived eval reports events back through `dioxus.send`, and Rust kicks actions with
//! tiny one-shot evals into `window.__wado.*`. This module is the only place that knows that.

use dioxus::prelude::*;
use wado_protocol::logfmt;

use crate::{
    persist,
    state::{Health, Ui, MAX_LOG_LINES},
};

/// The bridge script, assembled from the single-job `js/` files in load order.
///
/// Order matters twice: `core` defines the shared `W` object everything else hangs off, and
/// `lifecycle` must be last because it ends in a never-resolving await that keeps the eval —
/// and its `dioxus.send` channel — alive for the app's lifetime. `storage` precedes `theme`
/// because `theme` reads the saved scheme at load to avoid a flash of the default palette.
pub const JS: &str = concat!(
    include_str!("js/core.js"),
    "\n",
    include_str!("js/storage.js"),
    "\n",
    include_str!("js/theme.js"),
    "\n",
    include_str!("js/logs.js"),
    "\n",
    include_str!("js/video.js"),
    "\n",
    include_str!("js/stats.js"),
    "\n",
    include_str!("js/health.js"),
    "\n",
    include_str!("js/latency.js"),
    "\n",
    include_str!("js/webrtc.js"),
    "\n",
    // Before `relay.js`: that file calls `W.relayOn(...)` at load to register its handlers, and
    // this is what defines it. The dial at the bottom of `relay_link` is safe here — opening a
    // socket takes at least a tick, and `relay.js` registers synchronously in the same eval.
    include_str!("js/relay_link.js"),
    "\n",
    include_str!("js/relay.js"),
    "\n",
    include_str!("js/input_core.js"),
    "\n",
    include_str!("js/input_units.js"),
    "\n",
    include_str!("js/input_accel.js"),
    "\n",
    include_str!("js/input_coalesce.js"),
    "\n",
    include_str!("js/input_pointer.js"),
    "\n",
    include_str!("js/input_touch.js"),
    "\n",
    include_str!("js/input_scroll.js"),
    "\n",
    include_str!("js/input_keyboard.js"),
    "\n",
    include_str!("js/osk.js"),
    "\n",
    include_str!("js/refresh.js"),
    "\n",
    include_str!("js/overlay.js"),
    "\n",
    include_str!("js/viewport.js"),
    "\n",
    include_str!("js/wakelock.js"),
    "\n",
    include_str!("js/bar.js"),
    "\n",
    include_str!("js/settings.js"),
    "\n",
    include_str!("js/control.js"),
    "\n",
    include_str!("js/apps.js"),
    "\n",
    include_str!("js/pty.js"),
    "\n",
    include_str!("js/lifecycle.js"),
);

/// JS-encode a value: string → safe JS string literal, struct → JS object literal.
pub fn js(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Fire a `window.__wado.*` call and forget it.
pub fn call(code: String) {
    spawn(async move {
        let _ = document::eval(&code).await;
    });
}

/// Write the settings blob. Cheap enough to call on every change: one `JSON.stringify` and one
/// `localStorage` write, against a page that is otherwise decoding video.
pub fn save(saved: &persist::Saved) {
    call(format!("window.__wado.saveSettings({});", js(saved)));
}

/// Run the bridge and pump its events into signals until the page goes away.
///
/// Settings are loaded first and restored before anything else runs, so the session the user
/// starts uses their saved config rather than the defaults they never saw.
pub fn run(ui: Ui) {
    let mut live = ui.live;
    use_future(move || async move {
        let mut bridge = document::eval(JS);

        // The bridge's synchronous head has already run by the time a second eval is queued,
        // so these helpers exist. Load before connecting anything that depends on settings.
        if let Ok(v) = document::eval("return window.__wado.loadSettings();").await {
            if let Ok(saved) = serde_json::from_value::<persist::Saved>(v) {
                persist::restore(ui, saved);
            }
        }
        // Set even when nothing was stored: the flag means "loading is over", not "something
        // was found", and leaving it false would disable saving forever on a first run.
        live.loaded.set(true);
        // Restoring the settings only restored the *signals*. Anything mirrored on the JS
        // side has to be pushed, or the panel and the bridge disagree until the user happens
        // to touch each control. Every group with browser-side state owns one of these.
        // The theme is the one exception and needs no push: js/theme.js applied it from the
        // same blob during the bridge's synchronous head, which is precisely why it lives
        // there — doing it from here would repaint a page that is already correct.
        crate::debug::apply(ui);
        crate::ui::live::apply(ui);

        let server = (ui.set.server_addr)();
        let _ = document::eval(&format!("window.__wado.connectLogs({});", js(&server))).await;
        // Direct mode can ask now; relay mode has no socket until a room is open, so it asks
        // from relay.js on join_accepted instead. The address is passed explicitly because
        // W.server is only set as a side effect of connectLogs.
        let _ = document::eval(&format!("window.__wado.requestApps({});", js(&server))).await;

        while let Ok(msg) = bridge.recv::<serde_json::Value>().await {
            let Some(kind) = msg.get("type").and_then(|v| v.as_str()) else {
                continue;
            };
            let text = || {
                msg.get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let num = |k: &str| msg.get(k).and_then(|v| v.as_f64());
            let string = |k: &str| {
                msg.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            match kind {
                "status" => live.status.set(text()),
                "screen" => {
                    live.screen_w.set(num("w").unwrap_or(0.0) as u32);
                    live.screen_h.set(num("h").unwrap_or(0.0) as u32);
                    live.screen_dpr.set(num("dpr").unwrap_or(0.0));
                }
                "refresh" => live.refresh_hz.set(num("hz").map(|v| v as u32)),
                "osk" => {
                    let on = msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
                    live.osk_on.set(on);
                }
                "phase" => {
                    // Monotonic: a late stray message must not walk the indicator backwards.
                    let n = num("stage").unwrap_or(0.0) as u8;
                    if n == 0 || n > (live.conn_stage)() {
                        live.conn_stage.set(n);
                    }
                    live.conn_error.set(string("error"));
                }
                "stagebar" => live.stagebar.set(text()),
                "health" => {
                    live.health.set(Health {
                        state: string("state"),
                        side: string("side"),
                        detail: string("detail"),
                        fix: string("fix"),
                        need_kbps: num("needKbps"),
                        have_kbps: num("haveKbps"),
                        got_kbps: num("gotKbps"),
                    });
                }
                "stats" => {
                    live.fps.set(num("fps"));
                    live.ping.set(num("ping"));
                    live.jbuf.set(num("jbuf"));
                    if let Some(p) = num("decodeDropPct") {
                        live.decode_drop_pct.set(p);
                    }
                }
                "latency" => {
                    // Pipeline order, so the row reads left-to-right the way a frame and an
                    // input event actually travel. Stages the browser could not measure this
                    // tick are skipped rather than shown as a zero.
                    let stages = [
                        "capture", "encode", "queue", "net", "buf", "decode", "input",
                    ];
                    live.stages.set(
                        stages
                            .iter()
                            .filter_map(|k| num(k).map(|v| ((*k).to_string(), v)))
                            .collect(),
                    );
                    live.dropped
                        .set(msg.get("dropped").and_then(|v| v.as_u64()));
                }
                "apps" => {
                    live.apps.set(
                        msg.get("apps")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default(),
                    );
                }
                // A question, not a failure — so unlike "startFailed" this does not clear
                // session_on or throw the log console open. The socket is parked and waiting.
                "sessionAlive" => {
                    live.session_alive.set(Some((string("mode"), string("pipeline"))));
                }
                "sessionAliveCleared" => {
                    live.session_alive.set(None);
                }
                // **The session came up without the Start button being pressed.**
                //
                // `session_on` used to be set in exactly one place — `actions::start` — which was
                // fine while pressing Start was the only way a session began. It is not any more:
                // a page reload now takes back a session that survived (the `wado.watching`
                // crumb), and a reconnect rejoins one on its own. Neither goes through that
                // button, so the UI sat there with Start enabled and Stop greyed out over a live,
                // streaming session, and the viewer had to press Start to make the buttons agree
                // with the picture they were already looking at.
                //
                // Reported by the user 2026-09-13: *"start and stop button status is not updated
                // when page refresh occurs and I have to click start again to connect to already
                // connected session"*.
                "sessionOn" => {
                    live.session_on.set(true);
                }
                // The other direction, which had the same hole: a session stopped by the daemon —
                // the watchdog reaping it, or another viewer dropping it — left Stop enabled over
                // nothing.
                "sessionOff" => {
                    live.session_on.set(false);
                    live.clear_telemetry();
                }
                "encoder" => {
                    live.encoder_mode.set(string("mode"));
                    live.encoder_pipeline.set(string("pipeline"));
                }
                "log" => {
                    let line =
                        logfmt::parse(msg.get("line").and_then(|v| v.as_str()).unwrap_or(""));
                    let mut buf = live.logs.write();
                    buf.push(line);
                    let excess = buf.len().saturating_sub(MAX_LOG_LINES);
                    buf.drain(0..excess);
                }
                // Both failure paths open the logs, because the reason is always in there.
                "startFailed" => {
                    live.session_on.set(false);
                    live.console_open.set(true);
                    live.console_tab.set("logs".to_string());
                    live.clear_telemetry();
                }
                "giveup" => {
                    live.session_on.set(false);
                    live.console_open.set(true);
                    live.console_tab.set("logs".to_string());
                    live.clear_telemetry();
                    let _ = document::eval("window.__wado.stopSession();").await;
                    live.status.set("idle".to_string());
                }
                _ => {}
            }
        }
    });
}
