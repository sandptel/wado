//! The lane's hint card: at the spot the relay's 64 KB/s cap is felt — no picture, files on the
//! lane, a big shell dump — one card that says it is the network, not wado, names the cause when
//! it is known, and offers Retry direct (Decision Log 2026-10-27). Words: [`crate::lane`].
//!
//! One card at a time, the most serious first. Dismissed, a hint stays away until it is raised
//! afresh (a new episode, or new words).

use dioxus::prelude::*;

use crate::lane::{self, Spot};
use crate::{bridge, state::Ui};

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    let link = (live.link)();
    let mut dismissed = use_signal(Vec::<(String, String)>::new);
    if (ui.set.conn_mode)() != "relay" {
        return rsx! {};
    }
    let session = (live.session_on)();
    let mut raised: Vec<(Spot, String)> = Vec::new();
    // No picture: a session whose peer connection failed — the link badge's own `relayed`.
    if session && link.state == "relayed" {
        raised.push((Spot::Picture, String::new()));
    }
    for (spot, detail) in live.lane_hints.read().iter() {
        if let Some(s) = lane::Spot::from_js(spot) {
            raised.push((s, detail.clone()));
        }
    }
    // Forget dismissals of hints that have gone, so the next episode is shown.
    let live_keys: Vec<(String, String)> = raised
        .iter()
        .map(|(s, d)| (s.key().to_string(), d.clone()))
        .collect();
    if dismissed.read().iter().any(|d| !live_keys.contains(d)) {
        dismissed.write().retain(|d| live_keys.contains(d));
    }
    let Some((spot, detail)) = raised
        .into_iter()
        .find(|(s, d)| !dismissed.read().contains(&(s.key().to_string(), d.clone())))
    else {
        return rsx! {};
    };
    let h = lane::hint(spot, &link, &detail);
    let key = (spot.key().to_string(), detail.clone());
    let retry = match spot {
        Spot::Files => "window.__wado.files.retryDirect().catch(() => {});",
        _ => "window.__wado.resync();",
    };
    rsx! {
        div { class: "lanehint", role: "status", "aria-live": "polite", "data-spot": spot.key(),
            div { class: "lanehint-head",
                span { class: "lanehint-dot" }
                b { "{h.title}" }
                button {
                    class: "lanehint-x",
                    "aria-label": "Dismiss",
                    onclick: move |_| dismissed.write().push(key.clone()),
                    "×"
                }
            }
            p { "{h.body}" }
            if h.retry {
                button {
                    class: "btn",
                    onclick: move |_| bridge::call(retry.to_string()),
                    "Retry direct"
                }
            }
        }
    }
}
