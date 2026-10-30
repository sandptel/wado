//! The session's state while there is no picture, *in the picture's place*: how far the
//! connection got, what it is waiting for and for how long, and the two ways forward — Retry and
//! Leave. It replaces the floating "Connecting…" card and the "No picture" lane card, which sat
//! over the bezel and the deck (the user's recording, 2026-10-30).
//!
//! A session can sit here for a long time (a phone's hotspot where ICE never completes), so this
//! is a calm resting state, not a spinner: the hops, one plain sentence, a clock, and a Leave
//! that is always reachable — it takes this device out (the session keeps running on the
//! computer) and goes to the computer's page.
//!
//! Shown in both arrangements: over the picture's slot in the console, centred on the stage in
//! the cockpit (`surround.css`).

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

const HOPS: [&str; 4] = ["Relay", "Computer", "Session", "Picture"];

/// Whether the stage is showing nothing but black: no frame yet, or no path that can carry one.
pub fn waiting(ui: Ui) -> bool {
    let live = ui.live;
    if !(live.session_on)() {
        return false;
    }
    let st = (live.stage)();
    let link = (live.link)();
    st.out_w <= 0.0 || link.state == "relayed"
}

pub fn render(ui: Ui) -> Element {
    if !waiting(ui) {
        return rsx! {};
    }
    let live = ui.live;
    let link = (live.link)();
    let relay = (ui.set.conn_mode)() == "relay";
    let stage = (live.conn_stage)();
    let error = (live.conn_error)();
    let name = crate::profile::current_name(ui);
    let relayed = link.state == "relayed";
    let (title, said) = if relayed {
        (
            "No picture — there is no direct path".to_string(),
            crate::lane::hint(crate::lane::Spot::Picture, &link, "").body,
        )
    } else if !error.is_empty() {
        (format!("{name} did not answer"), error.clone())
    } else if link.state == "reconnecting" {
        ("Getting the picture back…".to_string(), link.detail())
    } else {
        let what = match stage {
            0 | 1 => format!("Reaching {name}…"),
            2 => format!("Starting the session on {name}…"),
            _ => "Waiting for the picture…".to_string(),
        };
        (what, link.detail())
    };
    // The hop it is on (relay mode); a direct-mode page has no relay hop to show.
    let on = if relayed { 3 } else { stage.min(3) } as usize;
    let stopped = relayed || !error.is_empty();
    rsx! {
        div { class: "sstate", role: "status", "aria-live": "polite",
            "data-tone": if stopped { "warn" } else { "work" },
            div { class: "sstate-card",
                if !stopped { span { class: "sstate-ring", "aria-hidden": "true" } }
                b { class: "sstate-title", "{title}" }
                if relay {
                    div { class: "hops sstate-hops",
                        for (i, hop) in HOPS.iter().enumerate() {
                            div { key: "{hop}",
                                class: if i < on { "hop done" } else if i == on && stopped { "hop stopped" } else if i == on { "hop active" } else { "hop" },
                                "{hop}"
                            }
                        }
                    }
                }
                p { class: "sstate-said", "{said}" }
                // Ticked by the page (`js/elapsed.js`) — no re-render per second.
                p { class: "sstate-clock mono", "data-elapsed": "{title}", "aria-hidden": "true" }
                div { class: "sstate-acts",
                    button {
                        class: "btn",
                        "data-act": "session.leave",
                        title: "Go to {name}'s page. The session keeps running there.",
                        onclick: move |_| crate::actions::leave(ui),
                        "Leave"
                    }
                    button {
                        class: "btn primary",
                        onclick: move |_| bridge::call("window.__wado.resync();".to_string()),
                        if relayed { "Retry direct" } else { "Retry" }
                    }
                }
            }
        }
    }
}
