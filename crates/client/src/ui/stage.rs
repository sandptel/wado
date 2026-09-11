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
    // Off unless asked for. It is the only chrome that sat across the top of the picture, and
    // on a phone that is precisely the strip an application puts its own controls in — with
    // it always on you could not see what you were tapping.
    let show_statusbar = debug::on(ui, "statusbar");

    // Deliberately NOT summed into one total: the server legs and the browser legs are
    // measured on clocks that were never synchronised, and `input` is a round trip while the
    // rest are one-way. Adding them would produce a confident number that means nothing.
    let stages = (live.stages)();
    let show_lat = on && debug::on(ui, "latency") && !stages.is_empty();
    let dropped = (live.dropped)().unwrap_or(0);

    // The server can be entirely healthy while the picture still stutters, because the
    // viewing device cannot decode what it asked for — measured here at 1080x2422/60, ~5%
    // of frames lost in the browser while the pump reported no stalls at all. It belongs
    // next to the other latency numbers and nowhere else: as a banner it appeared and
    // vanished above the video, reflowing the stage every time the rate crossed back over.
    let decode_drop = (live.decode_drop_pct)();

    rsx! {
        // Everything that floats over the picture lives in here with it. The video is
        // absolutely positioned to fill this box, and an absolutely positioned element paints
        // above its non-positioned siblings — so without this wrapper the video covered the
        // log and shell panels entirely. They rendered; they were simply behind it.
        div { id: "stage-video",
        if sw_encoding {
            div { class: "swbanner", "⚠ Software encoding — higher CPU use and latency" }
        }
        if show_statusbar {
            div { id: "stagebar",
                // No input hint here any more. It is a first-run explanation rather than a
                // status, and it is what forced this readout to span the full width — the
                // reason it covered the row an application puts its own controls in.
                span { "{(live.stagebar)()}" }
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
                // Server-side drops and device-side drops are different faults with
                // different fixes, so they are two readings rather than one total.
                span { class: "latstage latnote",
                    span { class: "latname", "device drop" }
                    span { class: "latval", "{decode_drop:.1}%" }
                }
            }
        }
        video { id: "wado-video", autoplay: true, playsinline: true, muted: true }
        }
        // Over the picture, not under it: as a sibling below the video it took height off
        // the stream and letterboxed it.
        {super::console::render(ui)}
    }
}
