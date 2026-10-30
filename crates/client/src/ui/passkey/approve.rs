//! The sheet's "Approve on the computer instead", while it stands: waiting for someone at the
//! computer (with what they run there, and a way to stop asking), or how it ended.

use dioxus::prelude::*;

use crate::{bridge, stepup::Approve};

pub fn status(a: Approve, computer: String) -> Element {
    match a.state.as_str() {
        "waiting" => rsx! {
            div { class: "pkappwait", role: "status",
                b { "Waiting for someone at {computer}" }
                p {
                    "There, run "
                    span { class: "mono", "wado passkey approve" }
                    " or answer in "
                    span { class: "mono", "wado tui" }
                    ". Only someone at the computer can say yes — nobody answering in 2 minutes counts as no."
                }
                button {
                    class: "btn",
                    onclick: move |_| bridge::call("window.__wado.passkeyCancel();".into()),
                    "Stop asking"
                }
            }
        },
        "approved" => rsx! {},
        _ => {
            let why = if a.message.is_empty() {
                "Not approved on the computer.".to_string()
            } else {
                a.message
            };
            rsx! { p { class: "pknote", role: "alert", "{why}" } }
        }
    }
}
