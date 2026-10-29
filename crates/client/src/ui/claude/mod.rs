//! Claude on the Computer page: a card per Claude Code session on that computer, Allow / Deny
//! on the one that asks, and "New Claude session" in a project folder.
//!
//! | file | job |
//! |---|---|
//! | [`card`] | one session's card, with its question |
//! | [`picker`] | the sheet that picks the folder a new session starts in |
//!
//! Reading the cards needs the `shells` grant only; everything that acts asks for the passkey
//! (`js/claude.js`, `crate::stepup`).

pub mod card;
pub mod picker;

use dioxus::prelude::*;

use crate::{bridge, claude::CARDS, state::Ui, ui::widgets::Icon};

/// The section's body (the Computer page draws its heading).
pub fn section(ui: Ui) -> Element {
    let name = crate::profile::current_name(ui);
    let mut copied = use_signal(|| false);
    let Some(st) = CARDS() else {
        return rsx! { div { class: "npempty", Icon { name: "term" } "Asking {name} about Claude…" } };
    };
    let cmd = "wado claude hooks install";
    let open = st.sessions.iter().filter(|s| s.state != "ended").count();
    rsx! {
        if !st.hooks {
            div { class: "card clhooks",
                b { "See Claude here" }
                p { "Status, questions and Allow / Deny for Claude Code on {name} appear here once its hooks are in. On {name}, run:" }
                div { class: "cmd",
                    code { "{cmd}" }
                    button {
                        class: if copied() { "iconbtn copied" } else { "iconbtn" },
                        "aria-label": if copied() { "Copied" } else { "Copy the command" },
                        onclick: move |_| {
                            bridge::call(format!("window.__wado.clipboardToPhone({});", bridge::js(&cmd)));
                            copied.set(true);
                            spawn(async move {
                                bridge::sleep(1400).await;
                                copied.set(false);
                            });
                        },
                        span { key: "{copied()}", class: "glyph", Icon { name: if copied() { "check" } else { "copy" } } }
                    }
                }
                p { class: "why", "It shows you the change to Claude's settings before it makes it." }
            }
        } else if open == 0 && st.sessions.is_empty() {
            div { class: "npempty", Icon { name: "term" } "No Claude sessions on {name} right now." }
        }
        div { class: "cllist",
            for c in st.sessions.clone() {
                card::Card { key: "{c.id}", c, compact: false }
            }
        }
        button {
            class: "btn clnew",
            onclick: move |_| crate::stepup::then("new", "window.__wado.claudePick();"),
            Icon { name: "plus" } "New Claude session"
        }
        if !st.tmux {
            p { class: "why", "tmux isn't installed on {name}, so a new session lives in a wado shell and ends with it. With tmux it keeps running, and you can attach at the desk." }
        }
    }
}
