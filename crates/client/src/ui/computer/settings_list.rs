//! "This computer": the four pages about the selected computer, each subtitled with what is set
//! there now. On its page, and inside the control centre during a session.

use dioxus::prelude::*;

use crate::{
    route,
    state::Ui,
    ui::{pages::Page, widgets::NavRow},
};

pub fn render(ui: Ui) -> Element {
    let s = ui.set;
    let live = ui.live;
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
    let pad = (live.host)()
        .trackpad
        .map(|t| {
            let way = if t.landscape { "Landscape" } else { "Upright" };
            let rail = match t.rail.as_str() {
                "keys" => "keys beside the pad",
                "actions" => "quick actions beside the pad",
                _ => "no rail",
            };
            format!("{way} · {rail} · {} quick actions", t.quick.len())
        })
        .unwrap_or_else(|| "Layout, quick actions, gestures".into());
    let hs = (live.hoststate)();
    let sound = match &hs {
        Some(h) => {
            let out = crate::ui::host::speaker(h)
                .map(|s| s.label.clone())
                .unwrap_or_else(|| "Speakers".into());
            let bt = h.bluetooth.as_ref().map(|b| {
                if b.powered {
                    "Bluetooth on"
                } else {
                    "Bluetooth off"
                }
            });
            [Some(out.as_str()), bt]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
        }
        None => "The computer's sound and Bluetooth".into(),
    };
    let conn = {
        let via = if (s.conn_mode)() == "relay" {
            "Via relay"
        } else {
            "Direct"
        };
        let wifi = hs
            .as_ref()
            .and_then(|h| h.wifi.as_ref())
            .and_then(|w| w.connected.clone())
            .map(|n| format!(" · Wi-Fi {n}"))
            .unwrap_or_default();
        let grants = (live.host)()
            .grants
            .map(|g| {
                let words = crate::ui::computer::grant::has_words(&g);
                if words.is_empty() {
                    " · this phone: view only".to_string()
                } else {
                    format!(" · this phone: {words}")
                }
            })
            .unwrap_or_default();
        format!("{via}{wifi}{grants}")
    };
    let go = move |p: Page| route::go(ui, p);
    rsx! {
        div { class: "list",
            NavRow { icon: "monitor", hue: "0D", title: "Display & session", sub: display, onopen: move |_| go(Page::DisplaySession) }
            NavRow { icon: "trackpad", hue: "0C", title: "Trackpad", sub: pad, onopen: move |_| go(Page::Trackpad) }
            NavRow { icon: "sound", hue: "0E", title: "Sound & devices", sub: sound, onopen: move |_| go(Page::SoundDevices) }
            NavRow { icon: "net", hue: "09", title: "Connection & trust", sub: conn, onopen: move |_| go(Page::ConnectionTrust) }
        }
    }
}
