//! "On this phone": how wado looks and feels on the device in hand — whichever computer it is
//! using. On home, and inside the control centre during a session.

use dioxus::prelude::*;

use crate::{
    route,
    state::Ui,
    ui::{pages::Page, widgets::NavRow},
};

pub fn render(ui: Ui) -> Element {
    let s = ui.set;
    let look = format!(
        "{} · {} corners · {}",
        (s.theme)(),
        (s.radius)(),
        crate::ui::pages::switcher::layout_name(&(s.placement)()).to_lowercase()
    );
    let input = format!(
        "{} · scroll {:.2}×{}",
        if (s.touch_mode)() == "touch" {
            "Raw touch"
        } else {
            "Gestures"
        },
        (s.scroll_speed)(),
        if (s.natural_scroll)() {
            " · natural"
        } else {
            ""
        }
    );
    let tiles = format!("{} tiles in the control centre", s.tiles.read().len());
    let go = move |p: Page| route::go(ui, p);
    rsx! {
        div { class: "list",
            NavRow { icon: "palette", hue: "0A", title: "Appearance", sub: look, onopen: move |_| go(Page::Appearance) }
            NavRow { icon: "hand", hue: "0C", title: "Input feel", sub: input, onopen: move |_| go(Page::InputFeel) }
            NavRow { icon: "apps", hue: "0E", title: "Quick tiles", sub: tiles, onopen: move |_| go(Page::Tiles) }
            NavRow { icon: "pulse", hue: "08", title: "Diagnostics", sub: "Readouts · health · server log", onopen: move |_| go(Page::Diagnostics) }
        }
    }
}
