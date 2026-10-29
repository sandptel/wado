//! The link badge: which path this device is on, always on screen (Decision Log 2026-10-27 — the
//! limited relay lane must never pass for the product). A dot and a word or two in the corner;
//! tap for what it means. During a healthy direct session it shrinks to the dot, so a game
//! keeps its corner. The veil over the picture during a recovery is [`super::reconnecting`].

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    let link = (live.link)();
    let mut open = use_signal(|| false);
    let session = (live.session_on)();
    // Direct mode has no relay path to describe; and before the first tick there is nothing yet.
    // Outside a session the path is said in words already — on each computer's card and in its
    // page's Link reading — so the corner stays clear for home's own head.
    if (ui.set.conn_mode)() != "relay" || link.state.is_empty() || !session {
        return rsx! {};
    }
    let quiet = session && link.state == "direct" && !open();
    let tone = link.tone().class();
    let label = link.label();
    let detail = link.detail();
    let can_retry = session && link.state == "relayed";
    rsx! {
        div { class: "linkbadge {tone}", class: if quiet { "quiet" },
            button {
                class: "linkpill",
                title: "{label}",
                "aria-expanded": "{open()}",
                onclick: move |_| open.toggle(),
                span { class: "linkdot" }
                if !quiet { span { class: "linklabel", "{label}" } }
            }
            if open() {
                div { class: "linkcard", role: "status",
                    p { b { "{label}" } }
                    p { "{detail}" }
                    if can_retry {
                        button {
                            class: "btn",
                            onclick: move |_| bridge::call("window.__wado.resync();".to_string()),
                            "Retry direct"
                        }
                    }
                }
            }
        }
    }
}
