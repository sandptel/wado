//! How far the connection got — the four hops, and which one it is stuck on — as a bar under the
//! Computer page's readings, shown only while dialling or when it stopped.
//!
//! Relay mode reaches video through four hops that fail for unrelated reasons: an unreachable
//! relay, a Remote ID with no daemon behind it, a session that would not start, ICE that never
//! completed. One status line made them look the same; this names the one it stopped at, and
//! the error under it says the fix.

use dioxus::prelude::*;

use crate::state::Ui;

const STAGES: [(&str, &str); 4] = [
    ("Relay", "the relay itself is reachable"),
    ("Daemon", "a wado daemon is online for this Remote ID"),
    ("Session", "the compositor and encoder started"),
    ("Video", "media is flowing"),
];

pub fn render(ui: Ui) -> Element {
    let relay = (ui.set.conn_mode)() == "relay";
    let stage = (ui.live.conn_stage)();
    let error = (ui.live.conn_error)();
    let failed = !error.is_empty();
    let starting = (ui.live.session_on)();
    // Joined and idle is not "connecting": the bar has said all it has to.
    if !relay || (stage >= 2 && !failed && !starting) {
        return rsx! {};
    }
    rsx! {
        div { class: if failed { "hops failed" } else { "hops" }, role: "status",
            for (i, (name, hint)) in STAGES.iter().enumerate() {
                div {
                    key: "{name}",
                    class: if (i as u8) < stage { "hop done" }
                           else if (i as u8) == stage && failed { "hop stopped" }
                           else if (i as u8) == stage { "hop active" }
                           else { "hop" },
                    title: "{hint}",
                    "{name}"
                }
            }
        }
        if failed { p { class: "connerr", "{error}" } }
    }
}
