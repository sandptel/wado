//! The console: an interactive shell and the server log, in one sheet over the picture.
//!
//! Two things decided the shape. They float rather than stack, because as siblings under the
//! video they took height off it and letterboxed the stream — the same fault the status bar
//! had. And they share one sheet with two tabs rather than opening separately, because a
//! phone has room for one panel at a time and two collapsed headers is two rows of nothing.
//!
//! The shell is a real terminal on a real PTY (see `server::pty` and `js/pty.js`), not a
//! box that runs one command and prints its lines. That is why this renders an empty div
//! and stops: xterm.js owns everything inside `#wado-term`, and Dioxus must never diff it.
//!
//! For the same reason the sheet is **hidden with a class rather than unmounted**. Removing
//! the node would take the terminal, its scrollback and its running shell with it every time
//! the console was closed.

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

/// Reveal the terminal and, the first time, ask the server for a shell.
fn show_shell() {
    bridge::call("window.__wado.ptyShow();".to_string());
}

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let open = (live.console_open)();
    let shell = (live.console_tab)() == "shell";
    let logs = live.logs.read().clone();

    rsx! {
        section {
            id: "console",
            class: if open { "open" } else { "shut" },

            div { class: "consoletabs",
                button {
                    class: if shell { "ctab on" } else { "ctab" },
                    onclick: move |_| {
                        live.console_tab.set("shell".to_string());
                        show_shell();
                    },
                    "Shell"
                }
                button {
                    class: if shell { "ctab" } else { "ctab on" },
                    onclick: move |_| live.console_tab.set("logs".to_string()),
                    "Logs"
                }
                span { class: "ctabgap" }
                button {
                    class: "ctab cclose",
                    "aria-label": "Close console",
                    onclick: move |_| live.console_open.set(false),
                    "✕"
                }
            }

            // Always in the tree, shown only on the shell tab. Empty on purpose — xterm.js
            // mounts into it and owns its children from then on.
            div {
                id: "wado-term",
                class: if shell { "termshow" } else { "termhide" },
            }

            if !shell {
                div { id: "wado-logwrap", class: "consolebody",
                    for (i, line) in logs.iter().enumerate() {
                        div { key: "{i}", class: "logline l-{line.level}",
                            span { class: "lts", "{line.ts}" }
                            " {line.text}"
                        }
                    }
                }
            }
        }
    }
}
