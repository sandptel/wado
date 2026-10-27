//! "Reconnecting…" over the picture while `js/netwatch.js` recovers from a network change — the
//! frozen last frame would otherwise read as a hang. Gone the moment the picture is back.

use dioxus::prelude::*;

use crate::state::Ui;

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    if !(live.session_on)() || (live.link)().state != "reconnecting" {
        return rsx! {};
    }
    rsx! {
        div { class: "reconnecting", "aria-live": "polite",
            span { class: "spin" }
            span { "Reconnecting…" }
        }
    }
}
