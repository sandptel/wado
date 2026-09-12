//! The choice offered when a session is already running.
//!
//! Its own file because it is one job with one trigger: `live.session_alive` being `Some`. It is
//! not part of `status`, which reports how far a connection got — this is not a report, it is the
//! connection waiting on an answer.
//!
//! Why a prompt rather than a default. Both answers destroy something a reasonable person might
//! want. Rejoining silently means a viewer who changed resolution, fps or scale gets none of it,
//! because the running session was built with the old ones and a Wayland output cannot be
//! resized (invariant #8). Dropping silently means a session with work open in it — a browser
//! mid-form, an editor, a build — is killed by someone reconnecting after a tunnel hiccup. There
//! is no safe default, so there is no default.

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

/// Human wording for the encoder report on the *running* session.
///
/// The point is to make "is this worth keeping?" answerable at a glance: a session that fell back
/// to software encoding is one you may well want to drop and restart.
fn describe(mode: &str, pipeline: &str) -> String {
    let base = match mode {
        "hardware" => "Hardware encoding",
        "software" => "⚠ Software encoding",
        _ => "Running",
    };
    match pipeline {
        "" => base.to_string(),
        p => format!("{base} · {p}"),
    }
}

pub fn render(ui: Ui) -> Element {
    let Some((mode, pipeline)) = (ui.live.session_alive)() else {
        return rsx! {};
    };

    rsx! {
        div { class: "rejoinprompt",
            h3 { "A session is already running" }
            p { class: "rejoindesc", "{describe(&mode, &pipeline)}" }
            p { class: "hint",
                "Rejoining keeps its windows and applications, but also its resolution, frame rate \
                 and scale — the settings on this device are not applied to a session that is \
                 already open."
            }
            div { class: "rejoinbtns",
                button {
                    class: "primary",
                    onclick: move |_| bridge::call("window.__wado.relayRejoin();".to_string()),
                    "Rejoin it"
                }
                button {
                    class: "danger",
                    onclick: move |_| bridge::call("window.__wado.relayDropStart();".to_string()),
                    "Drop & start new"
                }
            }
        }
    }
}
