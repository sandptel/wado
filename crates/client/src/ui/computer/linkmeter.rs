//! The link speed test, shown (Decision Log `2026-10-30`): a meter that sweeps while the burst is
//! pulled, then fills to what this link carried and settles into a verdict — ok, works worse, or
//! will not work — with the computer's own reasons, and how many devices one session here can
//! carry, as pips lighting one by one.
//!
//! The test itself is `js/linktest.js`; the judging is the daemon's (`server::linktest`).

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::Icon};

/// What a meter at full scale means, kbps: a link this fast carries anything wado sends.
const FULL_KBPS: f64 = 50_000.0;

fn mbps(k: u32) -> String {
    if k >= 10_000 {
        format!("{:.0}", k as f64 / 1000.0)
    } else {
        format!("{:.1}", k as f64 / 1000.0)
    }
}

#[component]
pub fn LinkMeter(ui: Ui, intent: String) -> Element {
    let m = (ui.live.multi)();
    let run = {
        let intent = intent.clone();
        move |_| bridge::call(format!("window.__wado.linkTest({});", bridge::js(&intent)))
    };
    let (tone, word) = match m.verdict.as_ref().map(|v| v.level.as_str()) {
        _ if m.testing => ("run", "Measuring…"),
        Some("ok") => ("ok", "Good to go"),
        Some("warn") => ("warn", "Works, worse"),
        Some("refuse") => ("bad", "Would not work"),
        _ => ("idle", "Not tested"),
    };
    let fill = m
        .verdict
        .as_ref()
        .filter(|_| !m.testing)
        .map(|v| (v.down_kbps as f64 / FULL_KBPS).clamp(0.04, 1.0))
        .unwrap_or(0.0);

    rsx! {
        div { class: "lmeter tone-{tone}", role: "group", "aria-label": "Link speed test",
            div { class: "lmhead",
                span { class: "lmkey", "Link test" }
                span { class: "lmverdict", role: "status", "aria-atomic": "true", i {} "{word}" }
                button {
                    class: "lmrun",
                    disabled: m.testing,
                    "aria-label": "Test this link again",
                    onclick: run,
                    Icon { name: "pulse" }
                    if m.verdict.is_some() { "Again" } else { "Test" }
                }
            }
            div { class: "lmtrack",
                div { class: if m.testing { "lmfill sweep" } else { "lmfill" }, style: "--f:{fill:.3}" }
            }
            if let Some(v) = m.verdict.clone().filter(|_| !m.testing) {
                div { class: "lmnums",
                    span { b { class: "num", "{mbps(v.down_kbps)}" } small { " Mbps down" } }
                    span { b { class: "num", "{v.rtt_ms}" } small { " ms round trip" } }
                    if v.session_kbps > 0 {
                        span { b { class: "num", "{mbps(v.session_kbps)}" } small { " Mbps stream" } }
                    }
                }
                div { class: "lmcap",
                    span { class: "pips", "aria-hidden": "true",
                        for i in 0..v.max_participants.max(v.suggested_max).min(8) {
                            i { key: "{i}", class: if i < v.max_participants { "on" } else { "" }, style: "--i:{i}" }
                        }
                    }
                    small {
                        "One session here carries {v.max_participants} device"
                        if v.max_participants != 1 { "s" }
                    }
                }
                if !v.reasons.is_empty() {
                    ul { class: "lmwhy",
                        for (i, r) in v.reasons.iter().enumerate() {
                            li { key: "{i}", style: "--i:{i}", "{r}" }
                        }
                    }
                }
            }
        }
    }
}
