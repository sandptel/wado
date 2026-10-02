//! The settings list: one row per page, each with a summary of what is set there now.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{pages::Page, widgets::NavRow},
};

pub fn render(ui: Ui) -> Element {
    let s = ui.set;
    let mut live = ui.live;
    let q = match (s.quality)().as_str() {
        "reactivity" => "Reactive",
        "quality" => "Sharp",
        "custom" => "Custom",
        _ => "Balanced",
    };
    let display = format!(
        "{} · {} fps · {}",
        (s.res)().replace('x', "×"),
        (s.fps)(),
        q
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
    let switcher = format!(
        "{} · {} · {} apps",
        (s.dial_orient)(),
        (s.dial_pos)().replace('-', " "),
        (s.dial_count)()
    );
    let pad = if (s.pad_on)() { "On" } else { "Off" }.to_string()
        + if (s.pad_mode)() == "pad" {
            " · real controller"
        } else {
            " · keys & mouse"
        };
    let look = format!("{} · {}", (s.theme)(), (s.radius)());
    let conn = crate::profile::capture(ui, String::new()).address();
    let host = (live.host)();
    let host_sub = if host.error.is_some() {
        "config.kdl has a mistake".to_string()
    } else {
        match host.limits.max_fps {
            Some(m) => format!("caps at {m} fps · config.kdl"),
            None => "Limits, pinned input · config.kdl".into(),
        }
    };

    let mut go = move |p: Page| live.cc_page.set(p);
    rsx! {
        div { class: "list",
            NavRow { icon: "monitor", hue: "0D", title: "Display & stream", sub: display, onopen: move |_| go(Page::Display) }
            NavRow { icon: "hand", hue: "0C", title: "Input & touch", sub: input, onopen: move |_| go(Page::Input) }
            NavRow { icon: "layers", hue: "0E", title: "Window switcher", sub: switcher, onopen: move |_| go(Page::Switcher) }
            NavRow { icon: "pad", hue: "0B", title: "Gamepad", sub: pad, onopen: move |_| go(Page::Gamepad) }
            NavRow { icon: "palette", hue: "0A", title: "Appearance", sub: look, onopen: move |_| go(Page::Appearance) }
            NavRow { icon: "net", hue: "09", title: "Connection", sub: conn, onopen: move |_| go(Page::Connection) }
            NavRow { icon: "sliders", hue: "0F", title: "This computer", sub: host_sub, onopen: move |_| go(Page::Host) }
            NavRow { icon: "pulse", hue: "08", title: "Diagnostics", sub: "Readouts · health · server log", onopen: move |_| go(Page::Diagnostics) }
        }
    }
}
