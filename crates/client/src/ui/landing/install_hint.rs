//! iOS has no install prompt, so Safari there is told how to put wado on the Home Screen — once
//! (`js/install_hint.js` decides, and marks it seen), dismissible. Elsewhere the browser offers
//! installing itself, from the manifest.

use dioxus::prelude::*;

use crate::{bridge, state::Ui, ui::widgets::Icon};

pub fn render(ui: Ui) -> Element {
    // Asked once the bridge has run (`loaded`), which is when `W.iosHint` exists.
    let mut show = use_signal(|| false);
    let mut leaving = use_signal(|| false);
    let mut asked = use_signal(|| false);
    use_effect(move || {
        if !(ui.live.loaded)() || *asked.peek() {
            return;
        }
        asked.set(true);
        spawn(async move {
            if let Ok(v) = document::eval("return !!window.__wado.iosHint;").await {
                show.set(v.as_bool().unwrap_or(false));
            }
        });
    });
    if !show() {
        return rsx! {};
    }
    rsx! {
        div { class: if leaving() { "card installhint leaving" } else { "card installhint" },
            role: "note",
            "aria-label": "Install wado",
            span { class: "ihicon", Icon { name: "share" } }
            div { class: "ihtext",
                b { "Keep wado on your Home Screen" }
                p { "Tap " b { "Share" } ", then " b { "Add to Home Screen" } ". It opens like an app, even offline." }
            }
            button {
                class: "iconbtn",
                "aria-label": "Dismiss",
                onclick: move |_| {
                    leaving.set(true);
                    spawn(async move {
                        bridge::sleep(180).await;
                        show.set(false);
                    });
                },
                Icon { name: "x" }
            }
        }
    }
}
