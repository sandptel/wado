//! The session shell: a command line and its output, beside the log.
//!
//! Its own section rather than part of the log panel because the two answer different
//! questions — the log is what the server is doing, this is what you asked it to do — and
//! interleaving a command's output with tracing makes both harder to read.
//!
//! Not a terminal emulator. Commands run without a PTY, so an editor, a pager or `top` will
//! not work; what does work is everything whose output is lines. A GUI application started
//! here still appears on the stream, because it inherits the session's `WAYLAND_DISPLAY`.

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

/// Send the draft command and clear the input.
fn submit(ui: Ui) {
    let mut set = ui.set;
    let mut live = ui.live;
    let command = (set.command)().trim().to_string();
    if command.is_empty() || (live.term_busy)() {
        return;
    }
    // Echoed so the output has something to belong to — without it a command that prints
    // nothing leaves no trace that anything ran at all.
    live.term.write().push((format!("$ {command}"), false));
    live.term_busy.set(true);
    bridge::call(format!(
        "window.__wado.relayExec({});",
        serde_json::to_string(&command).unwrap_or_else(|_| "\"\"".into())
    ));
    set.command.set(String::new());
}

pub fn render(ui: Ui) -> Element {
    let mut set = ui.set;
    let mut live = ui.live;
    let lines = live.term.read().clone();
    let busy = (live.term_busy)();

    rsx! {
        details { id: "term", open: (live.term_open)(),
            summary {
                onclick: move |e| {
                    e.prevent_default();
                    let open = (live.term_open)();
                    live.term_open.set(!open);
                },
                "Shell"
            }
            div { id: "wado-termwrap",
                for (i, (text, err)) in lines.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: if *err { "termline term-err" } else { "termline" },
                        "{text}"
                    }
                }
            }
            div { class: "termrow",
                input {
                    r#type: "text",
                    id: "wado-terminput",
                    placeholder: if busy { "running…" } else { "command — runs in the session, output below" },
                    autocomplete: "off",
                    autocapitalize: "off",
                    spellcheck: false,
                    value: "{(set.command)()}",
                    oninput: move |e| set.command.set(e.value()),
                    // Enter submits. The button is for touch, where there is no Enter key
                    // until the on-screen keyboard offers one.
                    onkeydown: move |e| if e.key() == Key::Enter { submit(ui) },
                }
                button { disabled: busy, onclick: move |_| submit(ui), "Run" }
            }
        }
    }
}
