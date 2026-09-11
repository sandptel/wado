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
    state::{Ui, MAX_LOG_LINES},
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
    include_str!("js/stats.js"),
    "\n",
    include_str!("js/latency.js"),
    "\n",
    include_str!("js/webrtc.js"),
    "\n",
    include_str!("js/relay.js"),
    "\n",
    include_str!("js/input_core.js"),
    "\n",
    include_str!("js/input_coalesce.js"),
    "\n",
    include_str!("js/input_pointer.js"),
    "\n",
    include_str!("js/input_touch.js"),
    "\n",
    include_str!("js/input_keyboard.js"),
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
        crate::debug::apply(ui);
        let _ = document::eval(&format!(
            "window.__wado.connectLogs({});",
            js(&(ui.set.server_addr)())
        ))
        .await;

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
                "stagebar" => live.stagebar.set(text()),
                "stats" => {
                    live.fps.set(num("fps"));
                    live.ping.set(num("ping"));
                    live.jbuf.set(num("jbuf"));
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
                    live.logs_open.set(true);
                    live.clear_telemetry();
                }
                "giveup" => {
                    live.session_on.set(false);
                    live.logs_open.set(true);
                    live.clear_telemetry();
                    let _ = document::eval("window.__wado.stopSession();").await;
                    live.status.set("idle".to_string());
                }
                _ => {}
            }
        }
    });
}
