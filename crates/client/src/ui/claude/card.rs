//! One Claude session: its project, what it is doing, the last thing it said — and when it asks
//! permission, the question with Deny and Allow right on the card. Tapping the card opens its
//! shell (behind the passkey). Allow carries a lock while this phone is locked: it will ask.

use dioxus::prelude::*;
use wado_protocol::claude::ClaudeSession;

use crate::{bridge, stepup::STEP, ui::widgets::Icon};

#[component]
pub fn Card(c: ClaudeSession, compact: bool) -> Element {
    let locked = !STEP().state.unlocked;
    let words = crate::claude::words(&c.state);
    let when = crate::profile::ago(c.since);
    let (s1, s2, s3) = (c.id.clone(), c.id.clone(), c.id.clone());
    let reach = c.tmux.is_some() || c.shell.is_some();
    rsx! {
        article {
            class: if compact { "ccard compact" } else { "ccard" },
            "data-state": "{c.state}",
            "aria-label": "Claude in {c.project}: {words}",
            button {
                class: "chead",
                disabled: !reach,
                title: if reach { "Open its shell" } else { "Started outside wado — no shell to open here" },
                onclick: move |_| bridge::call(format!("window.__wado.claudeOpen({});", bridge::js(&s1))),
                span { class: "cdot", "aria-hidden": "true" }
                span { class: "cmain",
                    b { "{c.project}" }
                    small { span { class: "cstate", "{words}" } " · {when}" }
                }
                if reach { span { class: "cgo", Icon { name: "term" } } }
            }
            if !c.message.is_empty() && c.ask.is_none() {
                p { class: "cmsg", "{c.message}" }
            }
            if let Some(ask) = c.ask.clone() {
                div { key: "{ask.id}", class: "cask", role: "group", "aria-label": "Claude asks to use {ask.tool}",
                    small { "Wants to use " b { "{ask.tool}" } }
                    code { class: "mono", "{ask.detail}" }
                    div { class: "cbtns",
                        button {
                            class: "btn",
                            onclick: {
                                let a = ask.id.clone();
                                move |_| bridge::call(format!("window.__wado.claudeAnswer({}, {}, false);", bridge::js(&s2), bridge::js(&a)))
                            },
                            Icon { name: "x" } "Deny"
                        }
                        button {
                            class: "btn primary",
                            onclick: {
                                let a = ask.id.clone();
                                move |_| bridge::call(format!("window.__wado.claudeAnswer({}, {}, true);", bridge::js(&s3), bridge::js(&a)))
                            },
                            if locked { Icon { name: "lock" } } else { Icon { name: "check" } }
                            "Allow"
                        }
                    }
                }
            }
            if !reach && !compact && c.state != "ended" {
                p { class: "chint", "Started outside wado — status only. Start the next one from wado to reach it from here." }
            }
        }
    }
}
