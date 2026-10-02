//! Toasts: brief notes over the picture that never move it.
//!
//! Two today. *Software encoding* is shown at the start of every session that has it — the
//! one-time telling invariant #5 asks for; the dock LED and the hero chip keep it visible after
//! the toast fades. *Config* surfaces a broken config.kdl, since that is the one thing a person
//! at the phone cannot otherwise see is wrong on the other machine.

use dioxus::prelude::*;

use crate::{state::Ui, ui::widgets::Icon};

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    let sw = (live.session_on)() && (live.encoder_mode)() == "software";
    let cfg_err = (live.host)().error;

    rsx! {
        div { class: "toasts", "aria-live": "polite",
            if sw {
                // Keyed by the session: a new session shows it again.
                div { key: "sw-{(live.applied)().len()}", class: "toast warn fade",
                    Icon { name: "alert" }
                    span { "Software encoding — no working GPU encoder. Higher CPU use and latency." }
                }
            }
            if let Some(e) = cfg_err {
                div { key: "cfg-{e.len()}", class: "toast bad fade",
                    Icon { name: "alert" }
                    span { "config.kdl not applied: {e}" }
                }
            }
        }
    }
}
