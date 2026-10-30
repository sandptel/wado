//! The hub's grant line (§3.2): one row, never hidden. Allowed and unlocked; allowed and locked,
//! with the unlock right there; or not allowed, with the exact command that allows it (the
//! Computer page's grant card). All three devices need the same `input` grant and passkey (D3).

use dioxus::prelude::*;

use crate::{
    bridge,
    reach::Reach,
    stepup::STEP,
    ui::{computer::grant::Grant, widgets::Icon},
};

#[component]
pub fn GrantLine(r: Reach, name: String) -> Element {
    if !r.may("input") {
        let device = r
            .grants()
            .map(|g| g.name)
            .unwrap_or_else(|| "<this phone>".into());
        let has = r
            .grants()
            .map(|g| crate::ui::computer::grant::has_words(&g))
            .unwrap_or_default();
        return rsx! {
            Grant {
                computer: name.clone(),
                device,
                token: "input",
                what: "its pointer, keyboard or a controller".to_string(),
                has,
                title: "Not allowed on this phone yet".to_string(),
            }
        };
    }
    let st = STEP.read().state.clone();
    let mins = (st.idle_s / 60).max(1);
    let (tone, icon, title, sub, act) = if st.unlocked {
        (
            "good",
            "check",
            "Allowed on this phone".to_string(),
            format!("Unlocked with your passkey · locks again after {mins} min idle"),
            None,
        )
    } else if st.registered {
        (
            "warn",
            "lock",
            "Allowed · locked".to_string(),
            "Unlock once and all three work until you leave them idle.".to_string(),
            Some("Unlock"),
        )
    } else {
        (
            "warn",
            "lock",
            "Allowed · needs a passkey".to_string(),
            format!("Your fingerprint, face or PIN becomes the key for {name}'s input."),
            Some("Add a passkey"),
        )
    };
    rsx! {
        div { class: "vigrant {tone}", role: "status",
            span { class: "vigdot", Icon { name: icon } }
            span { class: "vigtext", b { "{title}" } small { "{sub}" } }
            if let Some(label) = act {
                button {
                    class: "btn primary vigact",
                    onclick: move |_| bridge::call("window.__wado.passkeyFor('pad', () => {});".into()),
                    "{label}"
                }
            }
        }
    }
}
