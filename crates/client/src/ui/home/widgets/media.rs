//! The Media widget: what plays on its computer — cover, title, artist and where it plays — with
//! play/pause and next, and a thin progress line that runs as a clock between polls.

use dioxus::prelude::*;
use wado_protocol::{host::MediaOp, HostAction};

use crate::ui::{host, widgets::Icon};

use super::shell::{asking, Ctx};

#[component]
pub fn Media(cx: Ctx) -> Element {
    let Some(h) = cx.r.host.clone() else {
        return asking(&cx.name);
    };
    let Some(p) = h.media.first().cloned() else {
        return rsx! { div { class: "wnote quiet", Icon { name: "music" } "Nothing is playing on {cx.name}." } };
    };
    let out = h
        .audio
        .sinks
        .iter()
        .find(|s| s.name == h.audio.default_sink)
        .map(|s| s.label.clone())
        .unwrap_or_default();
    let art = p.art.clone().or(p.icon.clone());
    let frac = if p.length_ms > 0 {
        (p.position_ms as f64 / p.length_ms as f64).min(1.0)
    } else {
        0.0
    };
    let left = p.length_ms.saturating_sub(p.position_ms) as f64 / 1000.0;
    let (ui, pk) = (cx.ui, cx.w.host.clone());
    let (b1, b2, k1, k2) = (p.bus.clone(), p.bus.clone(), pk.clone(), pk);
    let title = if p.title.is_empty() {
        p.app.clone()
    } else {
        p.title.clone()
    };
    let sub = [p.artist.as_str(), out.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    rsx! {
        div { class: "wmedia",
            span { class: "cover",
                if let Some(a) = art { i { key: "{a}", style: "background-image:url(\"{a}\")" } }
                else { Icon { name: "music" } }
            }
            span { class: "mt",
                b { key: "{title}", class: "swapin", "{title}" }
                span { "{sub}" }
            }
            button { class: "iconbtn play", "aria-label": if p.playing { "Pause" } else { "Play" },
                onclick: move |_| host::act_on(ui, &k1, HostAction::Media { bus: b1.clone(), op: MediaOp::PlayPause }),
                span { key: "{p.playing}", class: "glyph", Icon { name: if p.playing { "pause" } else { "play" }, fill: true } }
            }
            button { class: "iconbtn", "aria-label": "Next", disabled: !p.can_next,
                onclick: move |_| host::act_on(ui, &k2, HostAction::Media { bus: b2.clone(), op: MediaOp::Next }),
                Icon { name: "next-f", fill: true }
            }
        }
        if p.length_ms > 0 {
            div { class: "thinbar",
                i { key: "{p.position_ms}-{p.playing}", class: if p.playing { "run" } else { "" }, style: "--from:{frac:.4};--left:{left:.1}s" }
            }
        }
    }
}
