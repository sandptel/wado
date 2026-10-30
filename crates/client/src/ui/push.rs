//! Connection & trust → Notifications: whether this phone hears from this computer when it is
//! not looking, and about what. One switch per kind, all on by default (user, `2026-10-30`).
//!
//! Says plainly what stands in the way: a browser without Web Push, notifications blocked in the
//! site settings, an iPhone where wado is not installed yet, or this phone's notifications
//! already coming from another computer (a browser holds one subscription at a time).

use dioxus::prelude::*;
use wado_protocol::push::PushKinds;

use crate::{bridge, push::PUSH, state::Ui, ui::widgets::SwitchRow};

fn set(kinds: PushKinds) {
    bridge::call(format!("window.__wado.pushKinds({});", bridge::js(&kinds)));
}

pub fn render(ui: Ui) -> Element {
    let p = PUSH();
    let name = crate::profile::current_name(ui);
    let ios = (ui.live.device_name)().to_lowercase().contains("iphone")
        || (ui.live.device_name)().to_lowercase().contains("ipad");
    let on = p.info.as_ref().is_some_and(|i| i.subscribed);
    let kinds = p.info.as_ref().map(|i| i.kinds).unwrap_or_default();
    let elsewhere = !p.from.is_empty() && p.from != p.here && !on;
    let blocked = p.permission == "denied";
    let can = p.support && !blocked && p.info.is_some() && !p.busy;
    rsx! {
        div { class: "card pushcard",
            div { class: "cardhead", "Notifications" }
            SwitchRow {
                title: format!("From {name}"),
                sub: if on { "On — even when wado is closed".to_string() } else { "Off".to_string() },
                on,
                disabled: !can,
                ontoggle: move |v: bool| {
                    bridge::call(if v { "window.__wado.pushOn();".into() } else { "window.__wado.pushOff();".into() });
                },
            }
            if on {
                div { class: "pushkinds",
                    SwitchRow {
                        title: "Claude needs permission".to_string(),
                        sub: "With Allow and Deny right on it".to_string(),
                        on: kinds.claude_permission,
                        ontoggle: move |v| set(PushKinds { claude_permission: v, ..kinds }),
                    }
                    SwitchRow {
                        title: "Claude finished".to_string(),
                        sub: "When a session is done and waiting".to_string(),
                        on: kinds.claude_done,
                        ontoggle: move |v| set(PushKinds { claude_done: v, ..kinds }),
                    }
                    SwitchRow {
                        title: format!("{name} is back online"),
                        sub: "After it was off or unreachable".to_string(),
                        on: kinds.online,
                        ontoggle: move |v| set(PushKinds { online: v, ..kinds }),
                    }
                    SwitchRow {
                        title: "Transfer finished".to_string(),
                        sub: "A big upload or download is done".to_string(),
                        on: kinds.transfer,
                        ontoggle: move |v| set(PushKinds { transfer: v, ..kinds }),
                    }
                    button { class: "btn", disabled: p.busy,
                        onclick: move |_| bridge::call("window.__wado.pushTest();".into()),
                        "Send a test"
                    }
                }
            }
            if !p.error.is_empty() {
                p { key: "{p.error}", class: "pknote", role: "alert", "{p.error}" }
            }
            if !p.support {
                p { class: "why",
                    if ios { "On iPhone and iPad, notifications need wado installed: Share → Add to Home Screen, then open it from there." }
                    else { "This browser can't receive notifications from a website." }
                }
            } else if ios && !p.standalone {
                p { class: "why", "On iPhone and iPad, notifications only arrive in the installed app: Share → Add to Home Screen, then turn them on from there." }
            } else if blocked {
                p { class: "why", "Notifications are blocked for wado in this browser — allow them in its site settings, then turn this on." }
            } else if elsewhere {
                p { class: "why", "This phone gets notifications from another computer now. Turning them on here moves them to {name}." }
            } else {
                p { class: "why", "wado can't tell you when {name} goes offline — nothing is left running to say so — only when it is back." }
            }
        }
    }
}
