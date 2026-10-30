//! The Computer page's Clipboard section: the whole Clipboard surface (`crate::ui::clipboard`),
//! this phone's two sharing switches for it, or — without the `clipboard` grant — what allows it.

use dioxus::prelude::*;

use crate::{
    reach::Reach,
    state::Ui,
    ui::clipboard::{switches::Switches, Clipboard},
};

use super::grant::{has_words, Grant};

#[component]
pub fn Clips(ui: Ui, pk: String, name: String, r: Reach) -> Element {
    if !r.may("clipboard") {
        let g = r.grants().unwrap_or_default();
        return rsx! {
            Grant {
                computer: name.clone(),
                device: g.name.clone(),
                token: "clipboard",
                what: "its clipboard",
                has: has_words(&g),
                title: "The clipboard needs permission",
            }
        };
    }
    rsx! {
        Switches { ui, pk: pk.clone(), name: name.clone(), may: true }
        Clipboard { ui, pk, name, selected: r.selected }
    }
}
