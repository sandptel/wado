//! The stage: the video and everything drawn around it.
//!
//! One rule governs this file. The software-encoding banner and the pipeline badge are **not**
//! debug views and are never gated by the debug master switch — invariant #5 requires the end
//! user to be told when the server fell back to software encoding, and a warning you can
//! switch off is not a warning. The fps/ping/latency readouts *are* debug views and go through
//! [`crate::debug::on`].

use dioxus::prelude::*;

use crate::{debug, state::Ui};

/// Label and CSS class for the active encode pipeline. `fallback` tiers are coloured as such:
/// the badge is how you notice you asked for hardware and did not get it.
fn badge(pipeline: &str) -> (&'static str, &'static str) {
    match pipeline {
        "vaapi-dmabuf" => ("⚡ zero-copy", "badge-good"),
        "vaapi-cpu" => ("HW · cpu-copy", "badge-warn"),
        "x264-cpu" => ("SW · x264", "badge-bad"),
        _ => ("", ""),
    }
}

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    let on = (live.session_on)();

    // Invariant #5: unconditional while software encoding is active.
    let sw_encoding = on && (live.encoder_mode)() == "software";
    let (badge_label, badge_class) = badge(&(live.encoder_pipeline)());
    let show_badge = on && !badge_label.is_empty();

    let show_fps = debug::on(ui, "fps");
    let show_ping = debug::on(ui, "ping");
    let stats_text = {
        let mut parts = Vec::new();
        if show_fps {
            parts.push((live.fps)().map_or("— fps".into(), |f| format!("{f:.0} fps")));
        }
        if show_ping {
            parts.push((live.ping)().map_or("— ms".into(), |p| format!("{p:.0} ms")));
            // Next to ping because the two answer different questions: ping is the network,
            // buf is how long the browser then sat on each frame before painting it.
            if let Some(b) = (live.jbuf)() {
                parts.push(format!("{b:.0} ms buf"));
            }
        }
        parts.join(" · ")
    };
    let show_stats = on && (show_fps || show_ping);

    // Deliberately NOT summed into one total: the server legs and the browser legs are
    // measured on clocks that were never synchronised, and `input` is a round trip while the
    // rest are one-way. Adding them would produce a confident number that means nothing.
    let stages = (live.stages)();
    let show_lat = on && debug::on(ui, "latency") && !stages.is_empty();
    let dropped = (live.dropped)().unwrap_or(0);
    let logs = live.logs.read().clone();

    // The server can be entirely healthy while the picture still stutters, because the
    // viewing device cannot decode what it asked for. Measured on this phone: 1080x2422 at
    // 60 fps dropped ~5% of frames in the browser while the pump reported no stalls and no
    // drops at all. Without this the only visible symptom is choppiness, which reads as a
    // server fault and sends you looking in the wrong place.
    let decode_drop = (live.decode_drop_pct)();
    let struggling = on && decode_drop >= 2.0;

    rsx! {
        if sw_encoding {
            div { class: "swbanner", "⚠ Software encoding — higher CPU use and latency" }
        }
        if struggling {
            div { class: "decodebanner",
                "⚠ This device is dropping {decode_drop:.0}% of frames — it cannot decode this \
                 fast enough. Lower the resolution or drop to 30 fps."
            }
        }
        div { id: "stagebar",
            span {
                "{(live.stagebar)()}"
                if on {
                    span { class: "kbdhint", " — tap to type · drag/scroll/long-press supported" }
                }
            }
            span {
                if show_badge {
                    span { class: "pipebadge {badge_class}", title: "active encode pipeline",
                        "{badge_label}"
                    }
                }
                if show_stats {
                    span { class: "stats", "{stats_text}" }
                }
            }
        }
        if show_lat {
            div { id: "wado-latency",
                for (label, ms) in stages.iter() {
                    span { key: "{label}", class: "latstage",
                        span { class: "latname", "{label}" }
                        span { class: "latval", "{ms:.1}" }
                    }
                }
                span { class: "latstage latnote",
                    span { class: "latname", "dropped" }
                    span { class: "latval", "{dropped}" }
                }
            }
        }
        video { id: "wado-video", autoplay: true, playsinline: true, muted: true }
        details { id: "logs", open: (live.logs_open)(),
            summary { "Logs" }
            div { id: "wado-logwrap",
                for (i, line) in logs.iter().enumerate() {
                    div { key: "{i}", class: "logline l-{line.level}",
                        span { class: "lts", "{line.ts}" }
                        " {line.text}"
                    }
                }
            }
        }
    }
}
