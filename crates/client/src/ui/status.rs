//! How far the connection actually got — the four hops, and which one you are stuck on.
//!
//! Separate from `connection` because that group asks *where to connect*; this one answers
//! *what happened when we tried*. Relay mode reaches video through four hops that fail for
//! unrelated reasons — an unreachable relay, a Remote ID with no daemon behind it, a session
//! that would not start, and ICE that never completed — and they are indistinguishable from
//! each other in a single status line. That ambiguity is what made "connection timed out"
//! mean four different things over one afternoon.

use dioxus::prelude::*;

use crate::state::Ui;

/// Ordered hops. The label is what succeeded; the hint is what a stall here means.
const STAGES: [(&str, &str); 4] = [
    ("Relay", "the relay itself is reachable"),
    ("Daemon", "a wado daemon is online for this Remote ID"),
    ("Session", "the compositor and encoder started"),
    ("Video", "media is flowing"),
];

pub fn render(ui: Ui) -> Element {
    // Relay-only: these four hops are the relay path's, and direct mode has no relay and no
    // Remote ID to be wrong about. ponytail: direct mode gets its own stages if it ever
    // needs them; today it is one HTTP call that either works or does not.
    if (ui.set.conn_mode)() != "relay" {
        return rsx! {};
    }
    let stage = (ui.live.conn_stage)();
    let error = (ui.live.conn_error)();
    let failed = !error.is_empty();

    rsx! {
        section { class: "group flat connstatus",
            label { "Connection status" }
            ol { class: "stages",
                for (i, (name, hint)) in STAGES.iter().enumerate() {
                    li {
                        key: "{name}",
                        // The first incomplete stage is the one that failed, if any did.
                        class: if (i as u8) < stage { "done" }
                               else if (i as u8) == stage && failed { "failed" }
                               else if (i as u8) == stage && stage > 0 { "active" }
                               else { "pending" },
                        span { class: "dot" }
                        span { class: "name", "{name}" }
                        span { class: "hint", "{hint}" }
                    }
                }
            }
            if failed {
                p { class: "connerr", "{error}" }
            } else if stage as usize >= STAGES.len() {
                p { class: "connok", "Streaming." }
            } else if stage == 0 {
                p { class: "hint", "Not connected yet — press Start." }
            }
        }
    }
}
