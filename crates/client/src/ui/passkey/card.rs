//! Connection & trust → Passkey: whether this phone has one on this computer, whether it is
//! unlocked, and the three things to do about it — add, replace (while unlocked), lock now.

use dioxus::prelude::*;

use crate::{bridge, stepup::STEP, ui::widgets::Icon};

/// A device name as `wado` wants it typed: quoted when it has spaces.
pub fn typed(name: &str) -> String {
    if name.contains(char::is_whitespace) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

pub fn render(device: String) -> Element {
    let st = STEP();
    let s = &st.state;
    let host = s
        .origin
        .split("://")
        .nth(1)
        .unwrap_or(&s.origin)
        .to_string();
    let (tone, words) = match (s.registered, s.unlocked) {
        (false, _) => ("off", "None yet".to_string()),
        (true, true) => ("on", "Unlocked".to_string()),
        (true, false) => ("lock", "Locked".to_string()),
    };
    rsx! {
        div { class: "card pkcard",
            div { class: "cardhead", "Passkey" }
            div { class: "pkrow",
                span { class: "pkdisc {tone}", Icon { name: if s.unlocked { "check" } else { "lock" } } }
                span { class: "pkwhat",
                    b { "{words}" }
                    small {
                        if !s.registered {
                            "Shells, file changes and the trackpad stay shut on this phone until it has one."
                        } else if s.unlocked {
                            "Shells, file changes and the trackpad are open. It locks after 15 min without use, or when you leave the app."
                        } else {
                            "Made on {host}. Your fingerprint, face or PIN opens shells, file changes and the trackpad."
                        }
                    }
                }
            }
            div { class: "pkacts row",
                if !s.registered {
                    button { class: "btn primary", disabled: !st.supported,
                        onclick: move |_| bridge::call("window.__wado.passkeyAdd();".into()),
                        "Add a passkey"
                    }
                } else if s.unlocked {
                    button { class: "btn",
                        onclick: move |_| bridge::call("window.__wado.passkeyLockNow();".into()),
                        Icon { name: "lock" } "Lock now"
                    }
                    button { class: "btn",
                        onclick: move |_| bridge::call("window.__wado.passkeyAdd();".into()),
                        "Replace"
                    }
                } else {
                    button { class: "btn primary",
                        onclick: move |_| bridge::call("window.__wado.passkeyThen('again', null);".into()),
                        "Unlock"
                    }
                }
            }
            if !st.supported {
                p { class: "why", "This browser can't use passkeys — open wado in Chrome, Safari, Edge or Firefox, with a screen lock set on this device." }
            } else if s.registered && !s.unlocked {
                p { class: "why", "Lost it, or a new phone? On the computer run " span { class: "mono", "wado passkey remove {device}" } ", then add a new one here." }
            }
        }
    }
}
