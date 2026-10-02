//! The session at a glance: which host, what shape, the verdict, and three live sparklines.
//!
//! The verdict is the conclusion and the sparklines are the evidence — numbers alone help only
//! someone who already knows what they should be.

use dioxus::prelude::*;

use crate::{state::Ui, ui::widgets::Icon};

/// An SVG path for `data` in a 100×22 box between `lo` and `hi`, as `(line, area, last point)`.
fn spark(data: &[f64], lo: f64, hi: f64) -> (String, String, (f64, f64)) {
    if data.len() < 2 {
        return (String::new(), String::new(), (100.0, 11.0));
    }
    let n = (data.len() - 1) as f64;
    let pts: Vec<(f64, f64)> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                i as f64 / n * 100.0,
                20.0 - ((v - lo) / (hi - lo)).clamp(0.0, 1.0) * 18.0,
            )
        })
        .collect();
    let line: String = pts
        .iter()
        .enumerate()
        .map(|(i, (x, y))| format!("{}{x:.1} {y:.1}", if i == 0 { "M" } else { "L" }))
        .collect();
    let area = format!("{line}L100 22L0 22Z");
    (line, area, *pts.last().unwrap_or(&(100.0, 11.0)))
}

#[component]
fn Meter(
    k: String,
    v: String,
    unit: String,
    data: Vec<f64>,
    lo: f64,
    hi: f64,
    hue: String,
) -> Element {
    let (line, area, (x, y)) = spark(&data, lo, hi);
    rsx! {
        div { class: "meter",
            div { class: "k", "{k}" }
            div { class: "v", "{v}" small { "{unit}" } }
            svg { view_box: "0 0 100 22", preserve_aspect_ratio: "none", style: "--hue:var(--base{hue})",
                path { class: "area", d: "{area}" }
                path { class: "line", d: "{line}" }
                circle { cx: "{x}", cy: "{y}", r: "2" }
            }
        }
    }
}

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    let s = ui.set;
    let on = (live.session_on)();
    let h = (live.health)();
    let sp = (live.spark)();
    let name = s
        .profiles
        .read()
        .get((s.profile)())
        .map(|p| p.display())
        .unwrap_or_else(|| crate::profile::default_name(ui));
    let c = crate::cfg::build(ui);
    let sw = on && (live.encoder_mode)() == "software";
    let (verdict, tone) = match (on, h.state.as_str()) {
        (false, _) => ("not streaming".to_string(), "idle"),
        (true, "" | "ok") => ("healthy".to_string(), "ok"),
        (true, state) => (h.side.clone(), state),
    };
    let fps = (live.fps)().map_or("—".into(), |f| format!("{f:.0}"));
    let ping = (live.ping)().map_or("—".into(), |p| format!("{p:.0}"));
    let (bw, bwu) = match (h.got_kbps, h.need_kbps) {
        (Some(g), Some(n)) if n > 0.0 => (
            format!("{:.1}", g / 1000.0),
            format!("/{:.1} Mb", n / 1000.0),
        ),
        (Some(g), _) => (format!("{:.1}", g / 1000.0), "Mb".into()),
        _ => ("—".into(), "Mb".into()),
    };
    let stage = (live.conn_stage)();

    rsx! {
        div { class: "hero",
            div { class: "herotop",
                span { class: "avatar", Icon { name: "server" } }
                div {
                    h2 { "{name}" }
                    div { class: "sub", "{c.width}×{c.height} · {c.fps} fps · {c.scale}×" }
                }
                span { class: "verdict {tone}", "{verdict}" }
            }
            // Invariant #5: not switchable, not dismissable — as long as it is true, it is here.
            if sw {
                div { class: "swchip", Icon { name: "alert" } "Software encoding — no working GPU encoder. Higher CPU use and latency." }
            }
            if on {
                div { class: "meters",
                    Meter { k: "frames", v: fps, unit: "fps", data: sp.fps.clone(), lo: 0.0, hi: c.fps as f64, hue: "0B" }
                    Meter { k: "ping", v: ping, unit: "ms", data: sp.ping.clone(), lo: 0.0, hi: 150.0, hue: "0C" }
                    Meter { k: "stream", v: bw, unit: bwu, data: sp.kbps.clone(), lo: 0.0, hi: h.need_kbps.unwrap_or(8000.0).max(1.0) * 1.2, hue: "0D" }
                }
                if !h.fix.is_empty() { p { class: "herofix", "{h.fix}" } }
            }
            div { class: "hops",
                for (i, label) in ["relay", "daemon", "session", "video"].iter().enumerate() {
                    span { key: "{label}", class: if (i as u8) < stage { "hop done" } else { "hop" }, "{label}" }
                }
            }
        }
    }
}
