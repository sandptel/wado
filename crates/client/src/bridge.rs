//! The JS bridge: the script itself, the helpers that poke it, and the pump that drains its
//! events into signals.
//!
//! Everything browser-only lives on the JS side (`js/`), driven through Dioxus `eval`: one
//! long-lived eval reports events back through `dioxus.send`, and Rust kicks actions with
//! tiny one-shot evals into `window.__wado.*`. This module is the only place that knows that.

use dioxus::prelude::*;
use wado_protocol::logfmt;

use crate::state::Spark;
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
    // Before `storage.js`, whose connect link rewrites the address: the shortcut's `?go=` is
    // taken out first.
    include_str!("js/deeplink.js"),
    "\n",
    include_str!("js/storage.js"),
    "\n",
    "W.SCHEME_LIST = ",
    include_str!("schemes.json"),
    ";\n",
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
    // Before `relay.js`, whose negotiation waits on it, and the pad's and the files' links.
    include_str!("js/ice_gather.js"),
    "\n",
    include_str!("js/webrtc.js"),
    "\n",
    // Before `relay.js`: that file calls `W.relayOn(...)` at load to register its handlers, and
    // this is what defines it. The dial at the bottom of `relay_link` is safe here — opening a
    // socket takes at least a tick, and `relay.js` registers synchronously in the same eval.
    // The envelope (`e2e_*`) before `relay_link.js`, whose hooks call into it.
    // The check words of a typed pairing — one list, shared with the daemon.
    "W.SAS_WORDS = `",
    include_str!("../../protocol/src/sas_words.txt"),
    "`.trim().split(/\\s+/);\n",
    // Before `relay_link.js`, which reports every request and reply to it.
    include_str!("js/ask_clock.js"),
    "\n",
    include_str!("js/e2e_keys.js"),
    "\n",
    include_str!("js/e2e_seal.js"),
    "\n",
    include_str!("js/e2e_handshake.js"),
    "\n",
    include_str!("js/relay_link.js"),
    "\n",
    // After `relay_link.js`: it pairs through `relayKick`, and the link and the envelope call it.
    include_str!("js/pair.js"),
    "\n",
    include_str!("js/relay.js"),
    "\n",
    // After `relay.js`: `seat.js` wraps the `__up` handler `relay.js` registers.
    include_str!("js/sheet.js"),
    "\n",
    include_str!("js/seat.js"),
    "\n",
    // After `seat.js`, for the same reason: it wraps the `__up` handler, and `saveSettings`.
    include_str!("js/config.js"),
    "\n",
    // The link badge, the network watch and the heartbeat, in that order: each wraps the `__up`
    // handler, and the heartbeat reads what `link_state` classified.
    include_str!("js/link_state.js"),
    "\n",
    include_str!("js/netwatch.js"),
    "\n",
    include_str!("js/device_status.js"),
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
    include_str!("js/input_lock.js"),
    "\n",
    include_str!("js/input_touch.js"),
    "\n",
    // After `input_touch`, which defines `W.windowDragAt`.
    include_str!("js/input_tap.js"),
    "\n",
    include_str!("js/gestures.js"),
    "\n",
    // After `relay.js` (for `W.relayOn`) and before any tap is taken.
    include_str!("js/targets.js"),
    "\n",
    include_str!("js/menu_sheet.js"),
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
    include_str!("js/clipboard.js"),
    "\n",
    include_str!("js/cliphost.js"),
    "\n",
    // After `cliphost.js`, whose relay handlers hand it the selected computer's copies.
    include_str!("js/clip_share.js"),
    "\n",
    include_str!("js/notify.js"),
    "\n",
    // The file manager. After `relay.js` (it wraps the `__down` handler) and `sheet.js`;
    // `files_link` first, as the others hang off the `W.files` it creates.
    include_str!("js/files_link.js"),
    "\n",
    include_str!("js/files_sha.js"),
    "\n",
    include_str!("js/files_icons.js"),
    "\n",
    include_str!("js/files_xfer.js"),
    "\n",
    include_str!("js/files_ui.js"),
    "\n",
    include_str!("js/files_panels.js"),
    "\n",
    include_str!("js/files_stream.js"),
    "\n",
    // Plyr's icon sprite (MIT), vendored: injected by files_player.js instead of fetched.
    "W.PLYR_SPRITE = `",
    include_str!("js/vendor/plyr.svg"),
    "`;\n",
    include_str!("js/files_player.js"),
    "\n",
    include_str!("js/files_exif.js"),
    "\n",
    // The viewer: the dispatcher first, then what every kind's module uses, then one module per
    // kind of file, then the frame that opens them (`W.files.view`).
    include_str!("js/viewer/kinds.js"),
    "\n",
    include_str!("js/viewer/bytes.js"),
    "\n",
    include_str!("js/viewer/card.js"),
    "\n",
    include_str!("js/viewer/menu.js"),
    "\n",
    include_str!("js/viewer/details.js"),
    "\n",
    include_str!("js/viewer/image.js"),
    "\n",
    include_str!("js/viewer/video.js"),
    "\n",
    include_str!("js/viewer/audio.js"),
    "\n",
    include_str!("js/viewer/pdf.js"),
    "\n",
    include_str!("js/viewer/text.js"),
    "\n",
    include_str!("js/viewer/json.js"),
    "\n",
    include_str!("js/viewer/csv.js"),
    "\n",
    include_str!("js/viewer/markdown.js"),
    "\n",
    include_str!("js/viewer/archive.js"),
    "\n",
    include_str!("js/viewer/font.js"),
    "\n",
    include_str!("js/viewer/info.js"),
    "\n",
    include_str!("js/viewer/shell.js"),
    "\n",
    // After `pty.js` (it chains the `pty_output` handler) and the files (it reads `W.files`).
    include_str!("js/lane_hint.js"),
    "\n",
    // The trackpad. After `relay_link.js` (it registers relay handlers and chains `__up`) and
    // `wakelock.js`; `pad_link` first, as the others call the `W.pad` it creates.
    include_str!("js/pad_link.js"),
    "\n",
    // Settings after `config.js` (it chains `config_state`), orientation before touch.
    include_str!("js/pad_prefs.js"),
    "\n",
    include_str!("js/kb_prefs.js"),
    "\n",
    include_str!("js/pad_orient.js"),
    "\n",
    include_str!("js/pad_touch.js"),
    "\n",
    include_str!("js/pad_gestures.js"),
    "\n",
    include_str!("js/pad_keys.js"),
    "\n",
    include_str!("js/pad_feel.js"),
    "\n",
    include_str!("js/pad_icons.js"),
    "\n",
    include_str!("js/pad_hints.js"),
    "\n",
    // The one owner of the pad's pointer listeners: gestures, then touch, then the pictures.
    include_str!("js/pad_pointer.js"),
    "\n",
    include_str!("js/pad_confirm.js"),
    "\n",
    include_str!("js/pad_coach.js"),
    "\n",
    include_str!("js/pad_actions.js"),
    "\n",
    include_str!("js/pad_rail.js"),
    "\n",
    include_str!("js/pad_settings.js"),
    "\n",
    include_str!("js/pad_why.js"),
    "\n",
    include_str!("js/pad_ui.js"),
    "\n",
    // The gamepad (docs/superpowers/specs/2026-10-30-virtual-input-design.md): one layout model and
    // one renderer for the screen with no session, the editor, the in-session overlay and the
    // console deck. After the pad files (it rides `W.pad` and reuses `W.padWhyShow`) and
    // `config.js` (its prefs chain `config_state`); `screen` before the editor modules that use it.
    include_str!("js/gpad/variants.js"),
    "\n",
    include_str!("js/gpad/model.js"),
    "\n",
    include_str!("js/gpad/place.js"),
    "\n",
    include_str!("js/gpad/render.js"),
    "\n",
    include_str!("js/gpad/thumb.js"),
    "\n",
    include_str!("js/gpad/controller.js"),
    "\n",
    include_str!("js/gpad/motion.js"),
    "\n",
    include_str!("js/gpad/input.js"),
    "\n",
    include_str!("js/gpad/send_session.js"),
    "\n",
    include_str!("js/gpad/send_pad.js"),
    "\n",
    include_str!("js/gpad/why.js"),
    "\n",
    include_str!("js/gpad/prefs.js"),
    "\n",
    include_str!("js/gpad/screen.js"),
    "\n",
    include_str!("js/gpad/edit.js"),
    "\n",
    include_str!("js/gpad/inspector.js"),
    "\n",
    include_str!("js/gpad/picker.js"),
    "\n",
    include_str!("js/gpad/comfort.js"),
    "\n",
    // The in-session overlay, on the same model.
    include_str!("js/gamepad.js"),
    "\n",
    // The Virtual input pages' live previews and the hub's miniatures (ui/computer/input/).
    include_str!("js/input_preview/mount.js"),
    "\n",
    include_str!("js/input_preview/touchpad.js"),
    "\n",
    include_str!("js/input_preview/keyboard.js"),
    "\n",
    include_str!("js/input_preview/gamepad.js"),
    "\n",
    include_str!("js/input_preview/hub.js"),
    "\n",
    // After the pad files: it chains the `pad_out` handler `pad_link` registers.
    include_str!("js/actions.js"),
    "\n",
    // After the files and pad files: it wraps `W.files.req`, `W.padOpen` and `W.pad.onMsg`.
    include_str!("js/passkey_errors.js"),
    "\n",
    include_str!("js/passkey.js"),
    "\n",
    include_str!("js/passkey_approve.js"),
    "\n",
    // After passkey.js: Allow, opening a session and starting one go through `passkeyThen`.
    include_str!("js/claude.js"),
    "\n",
    include_str!("js/push.js"),
    "\n",
    include_str!("js/capture.js"),
    "\n",
    include_str!("js/audio.js"),
    "\n",
    include_str!("js/host.js"),
    "\n",
    include_str!("js/online.js"),
    "\n",
    // After the envelope files: it reuses their helpers for its own per-link handshakes.
    include_str!("js/side_link.js"),
    "\n",
    include_str!("js/sessions.js"),
    "\n",
    // Several devices in one session: the speed test, joined rooms (after relay.js and
    // sessions.js, whose handlers they chain), and the other devices' pointers.
    include_str!("js/linktest.js"),
    "\n",
    include_str!("js/multi.js"),
    "\n",
    include_str!("js/presence.js"),
    "\n",
    // The Pocket Console (ui/surround): the slots, this device's console preferences, the
    // arrangement and its morph, then the deck's faces. After presence.js (the morph refits its
    // dots) and the input files (the deck sends through `W.coalesce`).
    include_str!("js/stage_fit.js"),
    "\n",
    include_str!("js/console_prefs.js"),
    "\n",
    // This device's text and interface size, applied before Rust renders anything.
    include_str!("js/ui_size.js"),
    "\n",
    include_str!("js/stage_morph.js"),
    "\n",
    include_str!("js/deck_pad.js"),
    "\n",
    include_str!("js/deck_keys.js"),
    "\n",
    include_str!("js/hero_flip.js"),
    "\n",
    include_str!("js/cbar.js"),
    "\n",
    include_str!("js/widget_flip.js"),
    "\n",
    include_str!("js/widget_drag.js"),
    "\n",
    include_str!("js/widget_touch.js"),
    "\n",
    include_str!("js/chrome.js"),
    "\n",
    include_str!("js/watchdog.js"),
    "\n",
    include_str!("js/autorate.js"),
    "\n",
    include_str!("js/caps.js"),
    "\n",
    // The installed app: the service worker that pins this client, and iOS's install hint.
    include_str!("js/app_sw.js"),
    "\n",
    include_str!("js/install_hint.js"),
    "\n",
    // The Android app's native side (android/), feature-detected: without `window.wadoNative`
    // these define `W.native = null` and do nothing. Last but one: they wrap functions and chain
    // relay handlers the files above define.
    include_str!("js/native.js"),
    "\n",
    include_str!("js/native_sync.js"),
    "\n",
    include_str!("js/native_feel.js"),
    "\n",
    include_str!("js/native_share.js"),
    "\n",
    include_str!("js/native_push.js"),
    "\n",
    include_str!("js/native_pad.js"),
    "\n",
    include_str!("js/native_passkey.js"),
    "\n",
    include_str!("js/native_size.js"),
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

/// Wait `ms` on the browser's clock — for a confirmation that undoes itself (a copied tick).
pub async fn sleep(ms: u32) {
    let _ = document::eval(&format!(
        "return new Promise((r) => setTimeout(() => r(1), {ms}));"
    ))
    .await;
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
        crate::ui::gamepad::apply(ui);

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
                // The Android app (`js/native.js`): its home opened a computer, or Back ran out.
                "nativeGo" => crate::native::go(ui, &string("key"), &string("go")),
                "nativeBack" => crate::native::back(ui),
                // A Back whose exit motion has run (`js/hero_flip.js`).
                "viewBack" => crate::route::back(ui),
                // Home's widgets: one removed after its exit, one dropped in a new place.
                "widgetRemove" => {
                    let mut s = ui.set;
                    crate::widget::remove(&mut s.widgets.write(), &string("id"));
                }
                "widgetPlace" => {
                    let mut s = ui.set;
                    let (from, to) = (
                        num("from").unwrap_or(0.0) as usize,
                        num("to").unwrap_or(0.0) as usize,
                    );
                    crate::widget::place(&mut s.widgets.write(), from, to);
                }
                "actions" => {
                    if let Some(list) = msg
                        .get("actions")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                    {
                        live.actions.set(list);
                    }
                }
                "ran" => {
                    let m = string("message");
                    if !m.is_empty() {
                        live.note.set(m);
                    }
                }
                "link" => {
                    if let Ok(l) = serde_json::from_value(msg.clone()) {
                        live.link.set(l);
                    }
                }
                // Kept out by another device's seat — and whether anything runs there.
                "seatBusy" => {
                    let clear = msg.get("clear").and_then(|v| v.as_bool()).unwrap_or(false);
                    live.seat_busy
                        .set((!clear).then(|| {
                            (string("holder"), msg.get("idle").and_then(|v| v.as_bool()))
                        }));
                }
                // Where the relay's limited lane is felt (`js/lane_hint.js`, `ui::lane_hint`).
                "laneHint" => {
                    let spot = string("spot");
                    let mut hints = live.lane_hints.write();
                    hints.retain(|(s, _)| *s != spot);
                    if msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false) {
                        hints.push((spot, string("detail")));
                    }
                }
                "screen" => {
                    live.screen_w.set(num("w").unwrap_or(0.0) as u32);
                    live.screen_h.set(num("h").unwrap_or(0.0) as u32);
                    live.screen_dpr.set(num("dpr").unwrap_or(0.0));
                    live.screen_phone
                        .set(msg.get("phone").and_then(|v| v.as_bool()).unwrap_or(false));
                    live.screen_css.set(string("css"));
                }
                "refresh" => live.refresh_hz.set(num("hz").map(|v| v as u32)),
                "pointer_lock" => {
                    let on = msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
                    live.pointer_lock.set(on);
                }
                "pad_edit" => {
                    let on = msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
                    live.pad_edit.set(on);
                    crate::ui::gamepad::apply(ui);
                }
                "osk" => {
                    let on = msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
                    live.osk_on.set(on);
                }
                "text_wanted" => {
                    let on = msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
                    live.text_wanted.set(on);
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
                // Refused for not being paired (`js/pair.js`), or cleared on a typed code.
                "unpaired" => live.unpaired.set(string("reason")),
                // A typed pairing's check words, until the computer answers (empty clears them).
                "pair_sas" => live.pair_sas.set(
                    msg.get("words")
                        .and_then(|w| w.as_array())
                        .map(|w| {
                            w.iter()
                                .filter_map(|x| x.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default(),
                ),
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
                    if let Some(k) = num("gotKbps") {
                        Spark::push(&mut live.spark.write().kbps, k);
                    }
                }
                "stats" => {
                    live.fps.set(num("fps"));
                    live.ping.set(num("ping"));
                    {
                        let mut sp = live.spark.write();
                        if let Some(f) = num("fps") {
                            Spark::push(&mut sp.fps, f);
                        }
                        if let Some(p) = num("ping") {
                            Spark::push(&mut sp.ping, p);
                        }
                    }
                    live.jbuf.set(num("jbuf"));
                    live.abuf.set(num("abuf"));
                    live.vmin.set(num("vmin"));
                    live.input_rt.set(num("input"));
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
                    let d = string("decoder");
                    if d != *live.decoder.peek() {
                        live.decoder.set(d);
                    }
                }
                "gpadPrefs" | "kbPrefs" | "padPrefs" => {
                    crate::ui::computer::input::prefs::on_event(kind, &msg);
                }
                "gpadSettings" => {
                    crate::route::go(ui, crate::ui::pages::Page::InputGamepad);
                }
                "hostConfig" => {
                    let state: wado_protocol::ConfigState = msg
                        .get("state")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    // The device's saved settings win over this browser's, once per link-up:
                    // they are what this device last chose, wherever it chose them.
                    if !(live.host_seen)() {
                        if let Some(saved) = state
                            .prefs
                            .as_deref()
                            .and_then(|p| serde_json::from_str::<persist::Saved>(p).ok())
                        {
                            // Where to connect stays this device's own: the daemon's copy can
                            // hold a tunnel URL that has since rotated, and restoring it would
                            // strand the next reload on a dead relay.
                            let s = ui.set;
                            let keep = (
                                (s.conn_mode)(),
                                (s.relay_url)(),
                                (s.remote_id)(),
                                (s.server_addr)(),
                            );
                            persist::restore(ui, saved);
                            let mut s = ui.set;
                            s.conn_mode.set(keep.0);
                            s.relay_url.set(keep.1);
                            s.remote_id.set(keep.2);
                            s.server_addr.set(keep.3);
                            crate::debug::apply(ui);
                            crate::ui::live::apply(ui);
                            crate::ui::gamepad::apply(ui);
                            crate::theme::apply(&(ui.set.theme)(), &(ui.set.theme_custom)());
                        }
                        live.host_seen.set(true);
                    }
                    live.host.set(state);
                }
                "shells" => {
                    let shells: Vec<wado_protocol::ShellInfo> = msg
                        .get("shells")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let hosts: Vec<String> = msg
                        .get("hosts")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    live.shell_active.set(num("active").unwrap_or(0.0) as u32);
                    live.shells.set(shells);
                    live.shell_hosts.set(hosts);
                }
                "shellActive" => {
                    live.shell_active.set(num("id").unwrap_or(0.0) as u32);
                    live.console_tab.set("shell".to_string());
                }
                "clipboard" => {
                    let text = string("text");
                    let mut clips = live.clips.write();
                    clips.retain(|c| *c != text);
                    clips.insert(0, text);
                    clips.truncate(crate::state::MAX_CLIPS);
                }
                "clipHistory" => {
                    let entries: Vec<wado_protocol::ClipEntry> = msg
                        .get("entries")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    // Pictures of entries no longer in the history go with them.
                    let here = crate::profile::current_key(ui);
                    live.clip_imgs.write().retain(|k, _| {
                        !k.starts_with(&format!("{here}\n"))
                            || entries
                                .iter()
                                .any(|e| *k == crate::state::clip_img_key(&here, &e.id))
                    });
                    live.clip_hist.set(crate::state::ClipHist {
                        loaded: true,
                        available: msg
                            .get("available")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                        entries,
                        error: string("error"),
                    });
                }
                // A picture for an image entry: the selected computer's, or a light link's (`key`).
                "clipImage" => {
                    let key = match string("key") {
                        k if k.is_empty() => crate::profile::current_key(ui),
                        k => k,
                    };
                    live.clip_imgs.write().insert(
                        crate::state::clip_img_key(&key, &string("id")),
                        string("data"),
                    );
                }
                "clipShare" => {
                    let prefs = msg
                        .get("prefs")
                        .and_then(|v| v.as_object())
                        .map(|m| {
                            m.iter()
                                .map(|(k, v)| {
                                    let b = |w: &str| {
                                        v.get(w).and_then(|x| x.as_bool()).unwrap_or(false)
                                    };
                                    (k.clone(), (b("pull"), b("push")))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    live.clip_prefs.set(prefs);
                }
                "clipChip" => live.clip_chip.set(
                    msg.get("chip")
                        .and_then(|v| serde_json::from_value(v.clone()).ok()),
                ),
                "clipSaid" => {
                    let n = live.clip_said.peek().n.wrapping_add(1);
                    let tone = match string("tone") {
                        t if t.is_empty() => "good".to_string(),
                        t => t,
                    };
                    live.clip_said.set(crate::state::ClipSaid {
                        n,
                        text: string("text"),
                        tone,
                    });
                }
                "clipError" => {
                    let mut h = live.clip_hist.write();
                    h.loaded = true;
                    h.error = string("text");
                }
                "notification" => {
                    let note = crate::state::Note {
                        id: num("id").unwrap_or(0.0) as u32,
                        app: string("app"),
                        summary: string("summary"),
                        body: string("body"),
                    };
                    let mut notes = live.notes.write();
                    notes.retain(|n| n.id != note.id);
                    notes.insert(0, note);
                    notes.truncate(crate::state::MAX_NOTES);
                }
                "hostState" => {
                    if let Some(st) = msg.get("state").and_then(|v| {
                        serde_json::from_value::<wado_protocol::HostState>(v.clone()).ok()
                    }) {
                        // Sound routed to this phone with no session: listen, so it is heard.
                        if crate::ui::host::on_phone(&st) && !(live.session_on)() {
                            call("window.__wado.listenStart();".to_string());
                        }
                        crate::profile::learn_host(ui, &st.info.hostname);
                        crate::profile::learn_info(ui, &crate::profile::current_key(ui), &st);
                        live.hoststate.set(Some(st));
                    }
                }
                "sessions" => {
                    let list: Vec<wado_protocol::SessionSummary> = msg
                        .get("sessions")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    live.sessions.set((string("here"), list));
                }
                // A light link's news about one computer: only the fields that changed.
                "side" => {
                    let key = string("key");
                    let mut side = live.side.write();
                    let e = side.entry(key).or_default();
                    e.state = string("state");
                    if let Some(v) = msg.get("host") {
                        e.host = serde_json::from_value(v.clone()).ok();
                    }
                    if let Some(v) = msg.get("config") {
                        e.config = serde_json::from_value(v.clone()).ok();
                    }
                    if let Some(v) = msg.get("clips") {
                        e.clips = serde_json::from_value(v.clone()).unwrap_or_default();
                    }
                    if let Some(v) = msg.get("actions") {
                        e.actions = serde_json::from_value(v.clone()).unwrap_or_default();
                    }
                }
                // The service worker: the version in use, an update waiting or refused (`js/app_sw.js`).
                "app" => live.app.set(crate::app_info::AppInfo::from_msg(&msg)),
                "online" => {
                    let key = string("key");
                    let n = num("daemons").unwrap_or(-1.0) as i32;
                    live.online.write().insert(key, n);
                }
                "audioBlocked" => live.note.set(
                    "Tap the sound tile to hear the session — this browser held the audio back."
                        .into(),
                ),
                "recording" => live
                    .recording
                    .set(msg.get("on").and_then(|v| v.as_bool()).unwrap_or(false)),
                "captureNote" => live.note.set(string("text")),
                // Several devices in one session (`js/multi.js`, `crate::multi`).
                "multiNote" => live.note.set(string("text")),
                "relayCaps" => {
                    live.multi.write().caps = msg
                        .get("caps")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                }
                "participants" => {
                    let mut m = live.multi.write();
                    m.list = msg
                        .get("list")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    m.you = num("you").unwrap_or(0.0) as u16;
                }
                "multiBusy" => live.multi.write().busy = (string("instance"), string("kind")),
                "linkTest" => {
                    let mut m = live.multi.write();
                    m.testing = string("phase") == "run";
                    if !m.testing {
                        m.verdict = msg
                            .get("verdict")
                            .and_then(|v| serde_json::from_value(v.clone()).ok());
                    }
                }
                // "Start new" found where the session goes: start it the ordinary way. `fresh`:
                // an older daemon could not say, so do what Start always did.
                "multiGo" => {
                    if msg.get("fresh").and_then(|v| v.as_bool()).unwrap_or(false) {
                        crate::ui::home::sessions::new(ui);
                    } else {
                        crate::actions::start(ui);
                    }
                }
                "notificationClosed" => {
                    let id = num("id").unwrap_or(0.0) as u32;
                    live.notes.write().retain(|n| n.id != id);
                }
                "clipboardSent" => {
                    let err = string("error");
                    live.note.set(if err.is_empty() {
                        "Phone clipboard sent to the session.".into()
                    } else {
                        err
                    });
                }
                "gesture" => crate::ui::gesture::run(ui, &string("action")),
                "passkey" => crate::stepup::on_event(ui, &msg),
                "claude" | "claudeDirs" | "claudeShell" | "claudePicker" => {
                    crate::claude::on_event(ui, kind, &msg)
                }
                "push" => crate::push::on_event(&msg),
                "shellMods" => {
                    let flag = |k: &str| msg.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
                    live.shell_mods.set((flag("ctrl"), flag("alt")));
                }
                "configRejected" => {
                    let m = msg.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    live.config_note.set(m.to_string());
                }
                "windows" => {
                    let list = msg
                        .get("windows")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let n = msg.get("workspace").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                    live.windows.set((list, n));
                }
                "alive" => {
                    let n = msg.get("n").and_then(|v| v.as_u64()).unwrap_or(0);
                    call(format!("window.__wado.alive({n});"));
                }
                // Auto bitrate changed the cap: re-apply to the running session (encoder only —
                // the output, its windows and apps stay as they are).
                "autorate" => {
                    let kbps = msg.get("kbps").and_then(|v| v.as_u64()).map(|v| v as u32);
                    live.auto_kbps.set(kbps);
                    if (live.session_on)() && (ui.set.auto_bitrate)() {
                        crate::actions::apply(ui);
                    }
                }
                // What this browser is: names the sound output that plays here.
                "caps" => {
                    live.device_name.set(string("device"));
                }
                "sizePrefs" => crate::size::on_event(&msg),
                "stageView" | "arrangement" | "consolePrefs" => {
                    crate::ui::surround::on_event(ui, kind, &msg)
                }
                "winMenu" => {
                    live.win_menu.set(msg.get("id").and_then(|v| v.as_u64()));
                }
                "rotate" => crate::actions::rotate(ui, &string("to")),
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
                    live.session_alive
                        .set(Some((string("mode"), string("pipeline"))));
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
                    // A join or take over that got this far is done.
                    live.multi.write().busy = Default::default();
                    // Not from this page's Start: a session resumed — after a reload, from the home
                    // page, or from another device. It is re-fitted to this device's shape: the
                    // same apps, at this screen's size and aspect.
                    if (live.applied)().is_empty() {
                        crate::actions::apply(ui);
                    }
                }
                // The other direction, which had the same hole: a session stopped by the daemon —
                // the watchdog reaping it, or another viewer dropping it — left Stop enabled over
                // nothing.
                "sessionOff" => {
                    live.session_on.set(false);
                    live.applied.set(String::new());
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
