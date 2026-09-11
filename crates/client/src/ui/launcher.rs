//! The launcher: pick an installed application, or type any command.
//!
//! One input, not two. The command box *is* the filter — typing narrows the list, picking a
//! row fills the box with what will actually run. A separate search field would mean two
//! places to look and a rule about which one wins.
//!
//! The free-text box is kept deliberately. It is also, per `CLAUDE.md`, the reason the control
//! plane binds localhost: an arbitrary command is remote code execution waiting for an open
//! port. A picker is the mechanism that could later narrow that to a known set of installed
//! applications — worth revisiting when the auth gate lands and the bind widens.

use dioxus::prelude::*;

use crate::{actions, state::Ui};

/// Rows to show at once. A phone screen cannot use more, and an unbounded list on a machine
/// with hundreds of desktop entries would push the Launch button off the sheet.
const MAX_ROWS: usize = 6;

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let on = (ui.live.session_on)();
    let command = (s.command)();
    let query = command.trim().to_lowercase();

    let apps = ui.live.apps.read();
    let matches: Vec<_> = apps
        .iter()
        .filter(|a| {
            query.is_empty()
                || a.name.to_lowercase().contains(&query)
                || a.exec.to_lowercase().contains(&query)
        })
        .take(MAX_ROWS)
        .cloned()
        .collect();

    // Once the box holds exactly what a row would set, the list has nothing left to offer and
    // showing it just covers the Launch button.
    let settled = matches.len() == 1 && matches[0].exec == command.trim();
    let show_list = !matches.is_empty() && !settled;

    rsx! {
        label { "Launch" }
        p { class: "hint",
            "Pick an application or type any command. Spawns into the running session — as "
            "many as you like."
        }
        input {
            r#type: "text",
            value: "{command}",
            placeholder: "search apps, or type a command",
            oninput: move |e| s.command.set(e.value()),
        }

        if show_list {
            ul { class: "applist",
                for app in matches.iter() {
                    li { key: "{app.exec}",
                        button {
                            class: "approw",
                            onclick: {
                                let exec = app.exec.clone();
                                move |_| s.command.set(exec.clone())
                            },
                            span { class: "appname", "{app.name}" }
                            span { class: "appexec", "{app.exec}" }
                        }
                    }
                }
            }
        }

        button {
            id: "launch", class: "wide",
            disabled: !on || command.trim().is_empty(),
            onclick: move |_| actions::launch(ui),
            "Launch into session"
        }
    }
}
