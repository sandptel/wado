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
        "{} · {}",
        crate::ui::pages::switcher::layout_name(&(s.placement)()),
        if (s.dock_pin)() {
            "dock pinned"
        } else {
            "dock fades"
        }
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

    let hs = (live.hoststate)();
    let sound = match &hs {
        Some(h) => format!(
            "{} · {}",
            if crate::ui::host::on_phone(h) {
                "On this device"
            } else {
                "On the computer"
            },
            match h.audio.streams.len() {
                0 => "nothing playing".to_string(),
                1 => "1 app".to_string(),
                n => format!("{n} apps"),
            }
        ),
        None => "The computer's apps and outputs".into(),
    };
    let net = match &hs {
        Some(h) => {
            let w = h.wifi.as_ref().map(|w| {
                w.connected.clone().unwrap_or_else(|| {
                    if w.enabled {
                        "Wi-Fi on".into()
                    } else {
                        "Wi-Fi off".into()
                    }
                })
            });
            let b = h.bluetooth.as_ref().map(|b| {
                if b.powered {
                    "Bluetooth on"
                } else {
                    "Bluetooth off"
                }
            });
            [w, b.map(str::to_string)]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
        }
        None => "The computer's connections".into(),
    };
    let mut go = move |p: Page| live.cc_page.set(p);
    rsx! {
        div { class: "list",
            NavRow { icon: "sound", hue: "0E", title: "Sound", sub: sound, onopen: move |_| go(Page::Sound) }
            NavRow { icon: "wifi", hue: "0C", title: "Wi-Fi & Bluetooth", sub: net, onopen: move |_| go(Page::Network) }
            NavRow { icon: "monitor", hue: "0D", title: "Display & stream", sub: display, onopen: move |_| go(Page::Display) }
            NavRow { icon: "hand", hue: "0C", title: "Input & touch", sub: input, onopen: move |_| go(Page::Input) }
            NavRow { icon: "layers", hue: "0E", title: "Workspaces & windows", sub: switcher, onopen: move |_| go(Page::Switcher) }
            NavRow { icon: "pad", hue: "0B", title: "Gamepad", sub: pad, onopen: move |_| go(Page::Gamepad) }
            NavRow { icon: "palette", hue: "0A", title: "Appearance", sub: look, onopen: move |_| go(Page::Appearance) }
            NavRow { icon: "net", hue: "09", title: "Connection", sub: conn, onopen: move |_| go(Page::Connection) }
            NavRow { icon: "sliders", hue: "0F", title: "This computer", sub: host_sub, onopen: move |_| go(Page::Host) }
            NavRow { icon: "server", hue: "0B", title: "Computers & relays", sub: format!("{} saved", s.profiles.read().len()), onopen: move |_| go(Page::Computers) }
            NavRow { icon: "pulse", hue: "08", title: "Diagnostics", sub: "Readouts · health · server log", onopen: move |_| go(Page::Diagnostics) }
        }
    }
}
