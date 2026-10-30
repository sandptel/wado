//! The console's top band: the computer's name beside the link words — or, while the session is
//! software-encoded, the "Software encoding" chip in the name's place (invariant 5: the status
//! is never dropped for lack of room). The link badge and the people chip are their own nodes
//! (`link_badge.rs`, `js/presence.js`), placed in this band by its slots.
//!
//! In the cockpit both fold away: the encoder state lives in the ⋯ LED there, as it always has.

use dioxus::prelude::*;

use crate::state::Ui;

pub fn render(ui: Ui) -> Element {
    let sw = (ui.live.encoder_mode)() == "software";
    let name = crate::profile::current_name(ui);
    let hue = crate::profile::hue((ui.set.profile)());
    rsx! {
        div { class: "cband", "aria-hidden": "true" }
        if sw {
            div { class: "cname enc", role: "status",
                span { class: "ldot" }
                span { "Software encoding" }
            }
        } else {
            div { class: "cname", title: "{name}",
                i { style: "--hue:var(--base{hue})" }
                span { "{name}" }
            }
        }
    }
}
