//! The passkey sheet: the one place this phone is asked for its passkey, or to add one.
//!
//! It rises over whatever asked — the Shell or Trackpad door, a file change, Claude's Allow —
//! says in plain words why, and runs the phone's own prompt. On success the lock's shackle lifts
//! before the sheet leaves, so the unlock is felt, not just inferred (`passkey.css`).
//!
//! It never traps the person (Decision Log `2026-10-30`): every wait gives up with a reason
//! (`js/passkey.js`), a failure offers Try again, and two ways round the passkey are always on
//! the sheet — approve this phone on the computer ([`approve`]), and the owner's switch to stop
//! asking it at all (`wado passkey require <device> off`). Not now always closes it.
//!
//! | file | job |
//! |---|---|
//! | [`card`] | Connection & trust → Passkey: add, unlock, replace, lock now; how long it stays unlocked |
//! | [`glyph`] | the lock drawn in two parts, so its shackle can open |

pub mod approve;
pub mod card;
pub mod glyph;

use dioxus::prelude::*;

use crate::{
    bridge, profile,
    state::Ui,
    stepup::{StepUp, STEP},
};

/// Why this door wants the passkey, in the words of what the person just tried.
fn lead(need: &str, name: &str) -> String {
    match need {
        "shell" => format!("Shells on {name} open with your passkey."),
        "pad" => format!("The trackpad and keyboard for {name} need your passkey."),
        "files" => {
            format!("Changing files on {name} needs your passkey. Looking around stays free.")
        }
        "allow" => "Letting Claude go ahead needs your passkey.".into(),
        "claude" => "Claude's shell opens with your passkey.".into(),
        "new" => "Starting Claude needs your passkey.".into(),
        "again" => format!("{name} locked itself while you were away. Unlock to carry on."),
        _ => format!(
            "Your screen lock — fingerprint, face or PIN — becomes the key for shells, file \
             changes and the trackpad on {name}."
        ),
    }
}

pub fn sheet(ui: Ui) -> Element {
    let st: StepUp = STEP();
    let name = profile::current_name(ui);
    // Held up a beat after an unlock, so the shackle is seen opening.
    let mut cheer = use_signal(|| false);
    let mut seen = use_signal(|| STEP.peek().opened);
    use_effect(move || {
        let opened = STEP.read().opened;
        if opened != *seen.peek() {
            seen.set(opened);
            cheer.set(true);
            spawn(async move {
                bridge::sleep(760).await;
                cheer.set(false);
            });
        }
    });
    let up = !st.need.is_empty() || cheer();
    let registered = st.state.registered;
    let elsewhere = registered && !st.state.origin.is_empty() && st.state.origin != st.origin;
    let fresh = matches!(st.need.as_str(), "paired" | "add");

    let title = if cheer() {
        "Unlocked".to_string()
    } else if !registered {
        if st.need == "paired" {
            "One last step: a passkey".into()
        } else {
            "Add a passkey".into()
        }
    } else {
        "Unlock with your passkey".into()
    };
    let body = if !registered && !fresh {
        format!(
            "This phone has no passkey on {name} yet. Add one now: your fingerprint, face or PIN \
             becomes the key, and nothing about it leaves this phone."
        )
    } else if fresh {
        "Nothing about it leaves this phone; the computer keeps only a public key.".into()
    } else {
        String::new()
    };
    let failed = !st.busy && !st.note.is_empty() && st.kind != "silent";
    let button = if st.busy && st.stage == "computer" {
        "Asking the computer…"
    } else if st.busy {
        "Waiting for your passkey…"
    } else if failed {
        "Try again"
    } else if registered && st.need != "add" {
        "Unlock"
    } else {
        "Add a passkey"
    };
    let can = st.supported && !st.busy && !elsewhere && !cheer();
    // The ways round the passkey: for any door, not for adding one on purpose.
    let hatches = !fresh && !cheer();
    let device = (ui.live.host)()
        .grants
        .map(|g| card::typed(&g.name))
        .unwrap_or_else(|| "<this device>".into());

    rsx! {
        div {
            class: if up { "pkscrim up" } else { "pkscrim" },
            onclick: move |_| bridge::call("window.__wado.passkeyCancel();".into()),
        }
        section {
            class: if up { "pksheet up" } else { "pksheet" },
            role: "dialog",
            "aria-modal": "true",
            "aria-labelledby": "pktitle",
            "aria-hidden": "{!up}",
            // Never `inert: bool`: Dioxus writes `inert="false"` for false, and any `inert` at all
            // makes the sheet click-through — every tap fell to the scrim and cancelled (2026-10-30).
            inert: (!up).then_some("true"),
            div { class: "grab" }
            div { class: if cheer() { "pkhead open" } else if st.busy { "pkhead busy" } else { "pkhead" },
                glyph::Lock {}
                h2 { id: "pktitle", "{title}" }
            }
            p { class: "pklead", "{lead(&st.need, &name)}" }
            if !body.is_empty() { p { class: "pkbody", "{body}" } }
            if !st.supported {
                div { class: "pkwarn",
                    b { "This browser can't use passkeys." }
                    p { "Open wado in Chrome, Safari, Edge or Firefox. On a phone, set a screen lock (PIN, fingerprint or face) first — that is what the passkey uses." }
                }
            } else if !st.platform && !registered {
                p { class: "pkbody dimmed", "No fingerprint, face or PIN is set up on this device — the browser will offer a security key or another phone instead. Setting a screen lock here is the easiest way." }
            }
            if elsewhere {
                div { class: "pkwarn",
                    b { "Your passkey belongs to another copy of wado" }
                    p { "It was made on " span { class: "mono", "{st.state.origin}" } ". Open wado there to unlock, or remove it on {name} with " span { class: "mono", "wado passkey remove" } " and add a new one here." }
                }
            }
            if !st.note.is_empty() {
                p { key: "{st.note}", class: "pknote", role: "alert", "{st.note}" }
            }
            div { class: "pkacts",
                button {
                    class: "btn primary",
                    disabled: !can,
                    onclick: move |_| {
                        let add = !STEP.peek().state.registered || STEP.peek().need == "add";
                        bridge::call(if add { "window.__wado.passkeyAdd();".into() } else { "window.__wado.passkeyGo();".into() });
                    },
                    "{button}"
                }
                if hatches && st.approve.as_ref().is_none_or(|a| a.state != "waiting") {
                    button {
                        class: "btn pkappr",
                        onclick: move |_| bridge::call("window.__wado.passkeyApprove();".into()),
                        "Approve on {name} instead"
                    }
                }
                button {
                    class: "btn",
                    onclick: move |_| bridge::call("window.__wado.passkeyCancel();".into()),
                    if fresh { "Later" } else { "Not now" }
                }
            }
            if hatches {
                if let Some(a) = st.approve.clone() { {approve::status(a, name.clone())} }
            }
            if hatches && (failed || st.approve.is_some()) {
                p { class: "pkfine pkoff",
                    "Passkey never works on this phone? On {name}, its owner can stop asking for it: "
                    span { class: "mono", "wado passkey require {device} off" }
                }
            }
            p { class: "pkfine", "{card::lasts_after_unlock(&st.state.stay, st.state.lock_on_leave)}" }
        }
    }
}
