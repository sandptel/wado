//! This phone's two switches for one computer: copy from it as things are copied there, and send
//! this phone's copies to it when wado comes back. Kept on the phone (`js/clip_share.js`), on by
//! default for a phone paired with a code that has the `clipboard` grant.

use dioxus::prelude::*;

use crate::{
    bridge,
    state::Ui,
    ui::widgets::{Icon, SwitchRow},
};

#[component]
pub fn Switches(ui: Ui, pk: String, name: String, may: bool) -> Element {
    let (pull, push) = ui
        .live
        .clip_prefs
        .read()
        .get(&pk)
        .copied()
        .unwrap_or_default();
    let set = move |pk: String, which: &'static str, on: bool| {
        bridge::call(format!(
            "window.__wado.clipSet({}, {}, {on});",
            bridge::js(&pk),
            bridge::js(&which)
        ));
    };
    let (pk1, pk2) = (pk.clone(), pk.clone());
    rsx! {
        div { class: "clipswitches",
            SwitchRow {
                title: "Copy from {name} automatically",
                sub: "What you copy there lands on this phone, ready to paste.",
                on: pull && may,
                disabled: !may,
                ontoggle: move |on| set(pk1.clone(), "pull", on),
            }
            SwitchRow {
                title: "Send this phone's copies automatically",
                sub: "When you come back to wado, what you copied here goes to {name}.",
                on: push && may,
                disabled: !may,
                ontoggle: move |on| set(pk2.clone(), "push", on),
            }
            p { class: "clipsecret",
                Icon { name: "lock" }
                span { "Passwords never travel: a copy a password manager marks as secret stays where it was made, and is kept in no history." }
            }
        }
    }
}
