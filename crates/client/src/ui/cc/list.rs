//! The control centre's settings list: two rows — *This computer ›* pushes the same four pages
//! its Computer page has, for the computer this session is on; *On this phone ›* the phone's own.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{pages::Page, widgets::NavRow},
};

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let name = crate::profile::current_name(ui);
    let hue = crate::profile::hue((ui.set.profile)());
    rsx! {
        div { class: "list",
            NavRow {
                icon: "monitor", hue: hue,
                title: "This computer",
                sub: "{name} · display, trackpad, sound, connection",
                onopen: move |_| live.cc_page.set(Page::ThisComputer),
            }
            NavRow {
                icon: "phone", hue: "0A",
                title: "On this phone",
                sub: "Appearance, input feel, gamepad, tiles, diagnostics",
                onopen: move |_| live.cc_page.set(Page::OnThisPhone),
            }
        }
    }
}
