//! The panel's command box: run anything, from the settings panel.
//!
//! The picker that used to be here is now [`super::drawer`], reached from the ⊞ on the bar —
//! browsing applications is a mid-session action and did not belong behind a settings panel.
//! What stays is the free-text line, for the case the drawer cannot serve: a command with
//! flags, or a machine whose application list the server could not read.
//!
//! It is also, per `CLAUDE.md`, the reason the control plane binds localhost: an arbitrary
//! command is remote code execution waiting for an open port. The drawer's grid is the
//! mechanism that could later narrow this to a known set of installed applications — worth
//! revisiting when the auth gate lands and the bind widens.

use dioxus::prelude::*;

use crate::{actions, state::Ui};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let on = (ui.live.session_on)();
    let command = (s.command)();

    rsx! {
        label { "Launch" }
        p { class: "hint",
            "Type any command. For installed applications, the ⊞ button on the bar opens the "
            "app drawer. Spawns into the running session — as many as you like."
        }
        input {
            r#type: "text",
            value: "{command}",
            placeholder: "command to run",
            oninput: move |e| s.command.set(e.value()),
        }
        button {
            id: "launch", class: "wide",
            disabled: !on || command.trim().is_empty(),
            onclick: move |_| actions::launch(ui),
            "Launch into session"
        }
    }
}
