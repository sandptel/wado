//! wado web client — a Dioxus (WASM) app that runs in the browser.
//!
//! It mirrors the old static `index.html`: a config panel (resolution / fps / quality
//! / launch command / advanced encoder knobs), Start/Stop, a live WebRTC video stage,
//! and a collapsible log panel fed by the server's `/events` SSE stream.
//!
//! The UI, state, config building, and log rendering are native Dioxus. The browser-only
//! pieces (fetch, WebRTC, `<video>.srcObject`, SSE, pagehide beacon) live in `bridge.js`,
//! driven through Dioxus `eval`: a long-lived bridge eval reports events back to Rust via
//! `dioxus.send`, and Rust kicks actions with tiny one-shot evals into `window.__wado.*`.

use dioxus::prelude::*;
use wado_protocol::{logfmt::LogLine, Quality, SessionConfig};

/// The bridge script (see module docs). Loaded as a static string so it needs no
/// `format!` brace-escaping; per-call arguments are passed via small separate evals.
const BRIDGE_JS: &str = include_str!("bridge.js");

/// Default server the client talks to. Editable in the UI; the dev server typically
/// runs the client on a different port and reaches the wado server here over CORS.
const DEFAULT_SERVER: &str = "http://127.0.0.1:8080";

/// Keep at most this many log lines in memory / the DOM.
const MAX_LOG_LINES: usize = 500;

fn main() {
    console_error_panic_hook::set_once();
    dioxus::launch(App);
}

/// JS-encode a value (string → safe JS string literal, struct → JS object literal).
fn js(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

#[component]
fn App() -> Element {
    // ── server + form state ──────────────────────────────────────────────────
    let mut server_addr = use_signal(|| DEFAULT_SERVER.to_string());
    let mut res = use_signal(|| "1280x720".to_string());
    let mut custom_w = use_signal(|| 1280u32);
    let mut custom_h = use_signal(|| 720u32);
    let mut fps = use_signal(|| 60u32);
    let mut quality = use_signal(|| "balanced".to_string());
    let mut bitrate = use_signal(|| 4000u32);
    let mut command = use_signal(|| "weston-terminal".to_string());
    let mut preset = use_signal(String::new);
    let mut keyframe = use_signal(String::new);

    // ── runtime state ────────────────────────────────────────────────────────
    let mut session_on = use_signal(|| false);
    let mut status = use_signal(|| "idle".to_string());
    let mut stagebar = use_signal(|| "No session.".to_string());
    let mut logs = use_signal(Vec::<LogLine>::new);
    let mut logs_open = use_signal(|| false);

    // ── bridge: run the JS bridge once, connect logs, drain events into signals ──
    use_future(move || async move {
        let mut bridge = document::eval(BRIDGE_JS);
        // Connect the log stream to the current server (functions are defined by the
        // synchronous head of the bridge eval, which runs before this next eval).
        let _ = document::eval(&format!("window.__wado.connectLogs({});", js(&server_addr())))
            .await;

        loop {
            let msg: serde_json::Value = match bridge.recv().await {
                Ok(v) => v,
                Err(_) => break,
            };
            let Some(kind) = msg.get("type").and_then(|v| v.as_str()) else { continue };
            let text = || msg.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            match kind {
                "status" => status.set(text()),
                "stagebar" => stagebar.set(text()),
                "log" => {
                    let line = msg.get("line").and_then(|v| v.as_str()).unwrap_or("");
                    let mut buf = logs.write();
                    buf.push(wado_protocol::logfmt::parse(line));
                    if buf.len() > MAX_LOG_LINES {
                        let excess = buf.len() - MAX_LOG_LINES;
                        buf.drain(0..excess);
                    }
                }
                "startFailed" => {
                    session_on.set(false);
                    logs_open.set(true);
                }
                "giveup" => {
                    session_on.set(false);
                    logs_open.set(true);
                    let _ = document::eval("window.__wado.stopSession();").await;
                    status.set("idle".to_string());
                }
                _ => {}
            }
        }
    });

    // Auto-scroll the log panel to the bottom when new lines arrive (unless the user
    // has scrolled up to read history).
    use_effect(move || {
        let _ = logs.read().len(); // subscribe to log changes
        let _ = document::eval(
            "var w=document.getElementById('wado-logwrap'); \
             if(w){var atBottom=w.scrollHeight-w.scrollTop-w.clientHeight<40; \
             if(atBottom) w.scrollTop=w.scrollHeight;}",
        );
    });

    // ── actions ──────────────────────────────────────────────────────────────
    let start = move |_| {
        let (width, height) = if res() == "custom" {
            (custom_w(), custom_h())
        } else {
            let r = res();
            let mut it = r.split('x');
            let w = it.next().and_then(|s| s.parse().ok()).unwrap_or(1280);
            let h = it.next().and_then(|s| s.parse().ok()).unwrap_or(720);
            (w, h)
        };
        let q = match quality().as_str() {
            "reactivity" => Quality::Reactivity,
            "quality" => Quality::Quality,
            "custom" => Quality::Custom { bitrate_kbps: bitrate() },
            _ => Quality::Balanced,
        };
        let cfg = SessionConfig {
            width,
            height,
            fps: fps(),
            quality: q,
            command: command(),
            preset: Some(preset()).filter(|p| !p.is_empty()),
            keyframe_interval: keyframe().trim().parse().ok(),
        };
        let server = server_addr();
        session_on.set(true);
        status.set("starting session…".to_string());
        spawn(async move {
            let code = format!("window.__wado.start({}, {});", js(&server), js(&cfg));
            let _ = document::eval(&code).await;
        });
    };

    let stop = move |_| {
        session_on.set(false);
        spawn(async move {
            let _ = document::eval("window.__wado.stopSession();").await;
            status.set("idle".to_string());
            stagebar.set("No session.".to_string());
        });
    };

    let on = session_on();
    let show_custom_res = res() == "custom";
    let show_custom_q = quality() == "custom";
    let log_items = logs.read().clone();

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }

        div { class: "app",
        div { id: "panel",
            h1 { "wado" }
            p { class: "hint", "Configure and start a streaming session." }

            label { "Server" }
            input {
                r#type: "text",
                value: "{server_addr}",
                oninput: move |e| server_addr.set(e.value()),
            }

            label { "Resolution" }
            select {
                value: "{res}",
                onchange: move |e| res.set(e.value()),
                option { value: "1280x720", "1280 × 720 (720p)" }
                option { value: "1920x1080", "1920 × 1080 (1080p)" }
                option { value: "1080x2400", "1080 × 2400 (phone portrait)" }
                option { value: "2400x1080", "2400 × 1080 (phone landscape)" }
                option { value: "custom", "Custom…" }
            }
            if show_custom_res {
                div { class: "row", style: "margin-top:8px;",
                    input {
                        r#type: "number", min: "16", step: "2", value: "{custom_w}",
                        oninput: move |e| if let Ok(v) = e.value().parse() { custom_w.set(v) },
                    }
                    input {
                        r#type: "number", min: "16", step: "2", value: "{custom_h}",
                        oninput: move |e| if let Ok(v) = e.value().parse() { custom_h.set(v) },
                    }
                }
            }

            label { "FPS" }
            select {
                value: "{fps}",
                onchange: move |e| if let Ok(v) = e.value().parse() { fps.set(v) },
                option { value: "30", "30" }
                option { value: "60", "60" }
                option { value: "120", "120" }
            }

            label { "Quality" }
            select {
                value: "{quality}",
                onchange: move |e| quality.set(e.value()),
                option { value: "reactivity", "Optimize reactivity (low latency)" }
                option { value: "balanced", "Balanced" }
                option { value: "quality", "Optimize image quality" }
                option { value: "custom", "Custom bitrate…" }
            }
            if show_custom_q {
                div { style: "margin-top:8px;",
                    label { style: "margin-top:0;", "Bitrate: {bitrate} kbps" }
                    input {
                        r#type: "range", min: "500", max: "20000", step: "500",
                        value: "{bitrate}", style: "width:100%;",
                        oninput: move |e| if let Ok(v) = e.value().parse() { bitrate.set(v) },
                    }
                }
            }

            label { "Application to launch" }
            input {
                r#type: "text",
                value: "{command}",
                placeholder: "e.g. mpv --fullscreen video.mp4",
                oninput: move |e| command.set(e.value()),
            }

            details {
                summary { "Advanced" }
                label { "x264 preset" }
                select {
                    value: "{preset}",
                    onchange: move |e| preset.set(e.value()),
                    option { value: "", "(from quality)" }
                    option { value: "ultrafast", "ultrafast" }
                    option { value: "superfast", "superfast" }
                    option { value: "veryfast", "veryfast" }
                    option { value: "faster", "faster" }
                }
                label { "Keyframe interval (frames)" }
                input {
                    r#type: "number", min: "1", placeholder: "(from quality)",
                    value: "{keyframe}",
                    oninput: move |e| keyframe.set(e.value()),
                }
            }

            div { class: "btns",
                button { id: "start", disabled: on, onclick: start, "Start" }
                button { id: "stop", disabled: !on, onclick: stop, "Stop" }
            }
            div { id: "status", "{status}" }
        }

        div { id: "stage",
            div { id: "stagebar", "{stagebar}" }
            video { id: "wado-video", autoplay: true, playsinline: true, muted: true }
            details { id: "logs", open: logs_open(),
                summary { "Logs" }
                div { id: "wado-logwrap",
                    for (i, line) in log_items.iter().enumerate() {
                        div { key: "{i}", class: "logline l-{line.level}",
                            span { class: "lts", "{line.ts}" }
                            " {line.text}"
                        }
                    }
                }
            }
        }
        }
    }
}
