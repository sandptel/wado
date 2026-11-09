//! A row that opens a page: tinted icon disc, title, a summary of what is set, chevron.

use dioxus::prelude::*;

use super::Icon;

#[component]
pub fn NavRow(
    icon: String,
    /// base16 slot for the disc, e.g. `"0D"`.
    hue: String,
    title: String,
    sub: String,
    onopen: EventHandler<()>,
    /// Coming to this OS (`crate::host_caps`): dimmed and badged; `onopen` says why.
    #[props(default)]
    later: bool,
) -> Element {
    rsx! {
        button { class: if later { "navrow soon" } else { "navrow" }, onclick: move |_| onopen.call(()),
            span { class: "disc", style: "--hue:var(--base{hue})", Icon { name: icon } }
            span { class: "navtext",
                b { "{title}" if later { " " span { class: "whenbadge later", "{crate::host_caps::SOON}" } } }
                small { "{sub}" }
            }
            Icon { name: "right" }
        }
    }
}
