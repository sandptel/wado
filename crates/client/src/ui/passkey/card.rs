//! Connection & trust → Passkey: whether this phone has one on this computer, whether it is
//! unlocked, the things to do about it — add, unlock, replace (while unlocked), lock now — and,
//! while unlocked, how long an unlock lasts here ([`stay`]).
//!
//! Every button answers on the card itself: busy while the computer and the phone's prompt
//! work, then the result or the reason it did not happen (`js/passkey.js` gives up after 20 s
//! on the computer, 60 s on the phone's prompt).
//!
//! While unlocked it also offers to stop asking this phone at all ([`require`]) — the owner's
//! way out for a phone whose passkey never works, with an honest word about what that means.

use dioxus::prelude::*;

use crate::{bridge, stepup::STEP, ui::widgets::Icon};

/// A device name as `wado` wants it typed: quoted when it has spaces.
pub fn typed(name: &str) -> String {
    // The computer may send it already typed (`Gate::typed`): quoted once, never twice.
    if name.contains(char::is_whitespace) && !name.starts_with('"') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// The choices, as the card says them: wire name, button label.
const STAYS: [(&str, &str); 4] = [
    ("15m", "15 min"),
    ("1h", "1 hour"),
    ("8h", "8 hours"),
    ("lock", "Until I lock"),
];

/// What the unlocked card says about how it ends.
fn lasts(stay: &str, leave: bool) -> String {
    let idle = match stay {
        "1h" => Some("1 hour"),
        "8h" => Some("8 hours"),
        "lock" => None,
        _ => Some("15 min"),
    };
    match (idle, leave) {
        (Some(t), true) => format!("It locks after {t} without use, or when you leave the app."),
        (Some(t), false) => format!("It locks after {t} without use."),
        (None, true) => "It stays unlocked until you lock it, or leave the app.".into(),
        (None, false) => "It stays unlocked until you lock it.".into(),
    }
}

/// The sheet's fine print: what an unlock will last, as this device chose it.
pub fn lasts_after_unlock(stay: &str, leave: bool) -> String {
    match (stay, leave) {
        ("1h", true) => "It locks again after 1 hour without use, and whenever you leave the app.",
        ("1h", false) => "It locks again after 1 hour without use.",
        ("8h", true) => "It locks again after 8 hours without use, and whenever you leave the app.",
        ("8h", false) => "It locks again after 8 hours without use.",
        ("lock", true) => "It stays unlocked until you lock it, or leave the app.",
        ("lock", false) => "It stays unlocked until you lock it.",
        (_, false) => "It locks again after 15 minutes without use.",
        _ => "It locks again after 15 minutes without use, and whenever you leave the app.",
    }
    .into()
}

/// How long an unlock lasts, and whether leaving the app locks it. Only while unlocked: the
/// daemon refuses a change from a locked device.
fn stay(stay: String, leave: bool) -> Element {
    let pick = |v: &str| {
        bridge::call(format!(
            "window.__wado.passkeyStay({});",
            bridge::js(&v.to_string())
        ))
    };
    let i = STAYS.iter().position(|(v, _)| *v == stay).unwrap_or(0);
    let lax = stay != "15m" || !leave;
    rsx! {
        div { class: "pkstay",
            b { id: "pkstaylbl", "Stay unlocked" }
            div { class: "seg", style: "--n:4;--i:{i}", role: "radiogroup", "aria-labelledby": "pkstaylbl",
                span { class: "knob" }
                for (v, label) in STAYS {
                    button {
                        key: "{v}",
                        class: if v == stay { "on" } else { "" },
                        role: "radio",
                        "aria-checked": "{v == stay}",
                        onclick: move |_| pick(v),
                        "{label}"
                    }
                }
            }
            div { class: "switchrow",
                div { class: "swtext",
                    b { "Lock when I leave the app" }
                    small { "Going to another app or the home screen locks it at once." }
                }
                button {
                    class: if leave { "sw on" } else { "sw" },
                    role: "switch",
                    "aria-checked": "{leave}",
                    "aria-label": "Lock when I leave the app",
                    onclick: move |_| bridge::call(format!(
                        "window.__wado.passkeyStay(window.__wado.passkey.state.stay || '15m', {});",
                        !leave
                    )),
                }
            }
            if lax {
                p { class: "pkhint", Icon { name: "info" }
                    span { "Anyone holding this phone can open shells, change files and use the trackpad while it is unlocked." }
                }
            }
        }
    }
}

/// "Ask for my passkey on this phone": on by default; off only while unlocked (the daemon
/// checks), and said plainly when it is off.
fn require(on: bool) -> Element {
    rsx! {
        div { class: "pkstay pkreq",
            div { class: "switchrow",
                div { class: "swtext",
                    b { "Ask for my passkey on this phone" }
                    small {
                        if on {
                            "Shells, file changes and the trackpad need your fingerprint, face or PIN."
                        } else {
                            "Off — this phone opens shells, file changes and the trackpad without it."
                        }
                    }
                }
                button {
                    class: if on { "sw on" } else { "sw" },
                    role: "switch",
                    "aria-checked": "{on}",
                    "aria-label": "Ask for my passkey on this phone",
                    onclick: move |_| bridge::call(format!("window.__wado.passkeyRequire({});", !on)),
                }
            }
            if !on {
                p { class: "pkhint", Icon { name: "info" }
                    span { "Anyone holding this phone can open shells, change files and use the trackpad on this computer. Turn it back on here, or on the computer." }
                }
            }
        }
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
    let asked = s.required;
    let (tone, words) = match (s.registered, s.unlocked) {
        _ if !asked => ("on", "Not asked".to_string()),
        (false, _) => ("off", "None yet".to_string()),
        (true, true) => ("on", "Unlocked".to_string()),
        (true, false) => ("lock", "Locked".to_string()),
    };
    // A daemon older than the choice sends no `stay`: nothing to choose there.
    let choosable = asked && s.unlocked && !s.stay.is_empty();
    let busy = st.busy;
    // The sheet, when up, says the same note itself; the card says it otherwise.
    let alert = st.need.is_empty();
    rsx! {
        div { class: "card pkcard", "aria-busy": "{busy}",
            div { class: "cardhead", "Passkey" }
            div { class: "pkrow",
                span { class: "pkdisc {tone}", Icon { name: if s.unlocked { "check" } else { "lock" } } }
                span { class: "pkwhat",
                    b { "{words}" }
                    small {
                        if !asked {
                            "This computer doesn't ask this phone for a passkey: shells, file changes and the trackpad are open."
                        } else if !s.registered {
                            "Shells, file changes and the trackpad stay shut on this phone until it has one."
                        } else if s.unlocked {
                            "Shells, file changes and the trackpad are open. {lasts(&s.stay, s.lock_on_leave)}"
                        } else {
                            "Made on {host}. Your fingerprint, face or PIN opens shells, file changes and the trackpad."
                        }
                    }
                }
            }
            if choosable { {stay(s.stay.clone(), s.lock_on_leave)} }
            if s.unlocked { {require(asked)} }
            div { class: "pkacts row",
                if !asked {
                } else if !s.registered {
                    button { class: "btn primary", disabled: !st.supported || busy,
                        onclick: move |_| bridge::call("window.__wado.passkeyAdd();".into()),
                        if busy { "Waiting for your passkey…" } else { "Add a passkey" }
                    }
                } else if s.unlocked {
                    button { class: "btn",
                        onclick: move |_| bridge::call("window.__wado.passkeyLockNow();".into()),
                        Icon { name: "lock" } "Lock now"
                    }
                    button { class: "btn", disabled: busy,
                        onclick: move |_| bridge::call("window.__wado.passkeyAdd();".into()),
                        if busy { "Waiting…" } else { "Replace" }
                    }
                } else {
                    button { class: "btn primary", disabled: busy,
                        onclick: move |_| bridge::call("window.__wado.passkeyThen('again', null);".into()),
                        if busy { "Waiting for your passkey…" } else { "Unlock" }
                    }
                }
            }
            if !st.note.is_empty() {
                p { key: "{st.note}", class: "pknote", role: if alert { "alert" } else { "status" }, "{st.note}" }
            }
            if !st.supported {
                p { class: "why", "This browser can't use passkeys — open wado in Chrome, Safari, Edge or Firefox, with a screen lock set on this device." }
            } else if s.registered && !s.unlocked {
                p { class: "why", "Lost it, or a new phone? On the computer run " span { class: "mono", "wado passkey remove {device}" } ", then add a new one here." }
            } else if s.unlocked && asked {
                p { class: "why", "To lock it from the computer: " span { class: "mono", "wado passkey lock {device}" } "." }
            }
            if asked && !s.unlocked {
                p { class: "why", "Passkey not working here? Use “Approve on the computer” on the passkey sheet, or on the computer run " span { class: "mono", "wado passkey require {device} off" } "." }
            }
        }
    }
}
