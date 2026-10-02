//! How far the connection got — the four hops, and which one it is stuck on.
//!
//! Relay mode reaches video through four hops that fail for unrelated reasons: an unreachable
//! relay, a Remote ID with no daemon behind it, a session that would not start, ICE that never
//! completed. One status line made them look the same; this names the one it stopped at.

use dioxus::prelude::*;

use crate::state::Ui;

const STAGES: [(&str, &str); 4] = [
    ("Relay", "the relay itself is reachable"),
    ("Daemon", "a wado daemon is online for this Remote ID"),
    ("Session", "the compositor and encoder started"),
    ("Video", "media is flowing"),
];

/// Circumference of the r=27 ring.
const RING: f64 = 169.6;

pub fn render(ui: Ui) -> Element {
    let relay = (ui.set.conn_mode)() == "relay";
    let stage = (ui.live.conn_stage)();
    let error = (ui.live.conn_error)();
    let failed = !error.is_empty();
    if !relay || (stage == 0 && !failed) {
        return rsx! {};
    }
    let done = (stage as f64 / 4.0).min(1.0);
    // Past the daemon with nothing starting is not "connecting": it is ready — the state
    // shell-only mode sits in.
    let starting = (ui.live.session_on)();
    let title = if failed {
        "Stopped"
    } else if stage >= 4 {
        "Streaming"
    } else if stage >= 2 && !starting {
        "Ready"
    } else {
        "Connecting…"
    };

    rsx! {
        div { class: if failed { "ring failed" } else { "ring" },
            svg { view_box: "0 0 64 64", "aria-hidden": "true",
                circle { class: "track", cx: "32", cy: "32", r: "27" }
                circle { class: "arc", cx: "32", cy: "32", r: "27",
                    style: "stroke-dasharray:{RING};stroke-dashoffset:{RING * (1.0 - done)}" }
            }
            div { class: "steps",
                b { "{title}" }
                for (i, (name, hint)) in STAGES.iter().enumerate() {
                    div {
                        key: "{name}",
                        class: if (i as u8) < stage { "step done" }
                               else if (i as u8) == stage && failed { "step failed" }
                               else if (i as u8) == stage && (starting || stage < 2) { "step active" }
                               else { "step" },
                        title: "{hint}",
                        i {} "{name}"
                    }
                }
            }
        }
        if failed { p { class: "connerr", "{error}" } }
    }
}
