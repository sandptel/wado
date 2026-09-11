//! The console: the session shell and the server log, in one sheet over the picture.
//!
//! Two things decided the shape. They float rather than stack, because as siblings under the
//! video they took height off it and letterboxed the stream — the same fault the status bar
//! had. And they share one sheet with two tabs rather than opening separately, because a
//! phone has room for one panel at a time and two collapsed headers is two rows of nothing.
//!
//! Sized in `dvh` and padded for the safe area: a fixed pixel height is wrong on every phone,
//! and a shell input that sits under the home indicator cannot be typed into.

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
    // nothing leaves no sign that anything ran.
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
    if !(live.console_open)() {
        return rsx! {};
    }
    let tab = (live.console_tab)();
    let shell = tab == "shell";
    let busy = (live.term_busy)();
    let term = live.term.read().clone();
    let logs = live.logs.read().clone();

    rsx! {
        section { id: "console",
            div { class: "consoletabs",
                button {
                    class: if shell { "ctab on" } else { "ctab" },
                    onclick: move |_| live.console_tab.set("shell".to_string()),
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

            if shell {
                div { id: "wado-termwrap", class: "consolebody",
                    for (i, (text, err)) in term.iter().enumerate() {
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
                        placeholder: if busy { "running…" } else { "command" },
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: false,
                        value: "{(set.command)()}",
                        oninput: move |e| set.command.set(e.value()),
                        // Enter submits; the button is for touch, where Enter only exists
                        // once the on-screen keyboard offers it.
                        onkeydown: move |e| if e.key() == Key::Enter { submit(ui) },
                    }
                    button { disabled: busy, onclick: move |_| submit(ui), "Run" }
                }
            } else {
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
