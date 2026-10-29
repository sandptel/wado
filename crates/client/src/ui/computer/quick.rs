//! Quick: the computer's volume and brightness, and three toggles — keep awake, Wi-Fi,
//! Bluetooth. Long-press a toggle (or right-click) for its page.
//!
//! A toggle that turns on changes shape as well as colour (the Morphing Toggle Rule) and its
//! icon dips and pops back. Wi-Fi never goes off on one tap: it asks first, in the same words the
//! Connection & trust page uses, because it can cut the very link this phone is using.

use dioxus::prelude::*;
use wado_protocol::{HostAction, HostState};

use crate::{
    reach::Reach,
    route,
    state::Ui,
    ui::{
        computer::grant::{has_words, Grant},
        host,
        pages::{network::WifiOff, Page},
        widgets::{Icon, Slider},
    },
};

#[component]
pub fn Quick(ui: Ui, pk: String, name: String, h: HostState, r: Reach) -> Element {
    let mut ask_wifi = use_signal(|| false);
    if !r.may("host") {
        let g = r.grants().unwrap_or_default();
        return rsx! {
            Grant {
                computer: name.clone(),
                device: g.name.clone(),
                token: "host",
                what: "its sound, media, brightness or radios",
                has: has_words(&g),
            }
        };
    }
    let speaker = host::speaker(&h).cloned();
    let k = pk.clone();
    let act = move |a: HostAction| host::act_on(ui, &k, a);
    let (a1, a2, a3, a4, a5) = (
        act.clone(),
        act.clone(),
        act.clone(),
        act.clone(),
        act.clone(),
    );
    let owner = r.may("settings");
    let wifi = h.wifi.clone();
    let wifi_on = wifi.as_ref().is_some_and(|w| w.enabled);
    let wifi_sub = match &wifi {
        None => "None here".to_string(),
        Some(w) if !w.enabled => "Off".into(),
        Some(w) => w.connected.clone().unwrap_or_else(|| "On".into()),
    };
    let bt = h.bluetooth.clone();
    let bt_on = bt.as_ref().is_some_and(|b| b.powered);
    let bt_sub = match &bt {
        None => "None here".to_string(),
        Some(b) if !b.powered => "Off".into(),
        Some(b) => b
            .devices
            .iter()
            .find(|d| d.connected)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "On".into()),
    };
    let uplink = wifi.as_ref().is_some_and(|w| w.uplink);

    rsx! {
        if let Some(s) = speaker {
            div { class: "volrow",
                button {
                    class: if s.muted { "iconbtn muted" } else { "iconbtn" },
                    "aria-label": if s.muted { "Unmute {name}" } else { "Mute {name}" },
                    "aria-pressed": "{s.muted}",
                    onclick: move |_| a1(HostAction::SinkMute { id: s.id, muted: !s.muted }),
                    span { key: "{s.muted}", class: "glyph", Icon { name: if s.muted { "mute" } else { "sound" } } }
                }
                Slider {
                    label: "Volume",
                    value: s.volume.min(1.0) as f64,
                    shown: if s.muted { "muted".to_string() } else { String::new() },
                    onchange: move |v: f64| a2(HostAction::SinkVolume { id: s.id, volume: v as f32 }),
                }
            }
        }
        match h.brightness {
            Some(b) => rsx! {
                Slider {
                    label: "Brightness",
                    icon: "sun",
                    value: b as f64,
                    step: 0.1,
                    ticks: 10,
                    onchange: move |v: f64| a3(HostAction::Brightness { level: v as f32 }),
                }
            },
            None => rsx! {
                div { class: "npempty", Icon { name: "sun" } "{name} has no screen brightness wado can set." }
            },
        }
        div { class: "qtiles",
            Toggle {
                icon: "eye", title: "Keep awake", on: h.awake,
                sub: if h.awake { "Won't sleep" } else { "Sleeps as usual" },
                onflip: move |_| a4(HostAction::KeepAwake { on: !h.awake }),
            }
            Toggle {
                icon: "wifi", title: "Wi-Fi", on: wifi_on, sub: wifi_sub,
                disabled: wifi.is_none() || !owner,
                onflip: move |_| if wifi_on { ask_wifi.set(true) } else { host::act_on(ui, &pk, HostAction::WifiRadio { on: true, minutes: None }) },
                onmore: move |_| route::go(ui, Page::ConnectionTrust),
            }
            Toggle {
                icon: "bt", title: "Bluetooth", on: bt_on, sub: bt_sub,
                disabled: bt.is_none(),
                onflip: move |_| a5(HostAction::BtPower { on: !bt_on }),
                onmore: move |_| route::go(ui, Page::SoundDevices),
            }
        }
        if wifi.is_some() && !owner {
            p { class: "why", "Only the owner device can change {name}'s Wi-Fi." }
        }
        if ask_wifi() {
            WifiOff { uplink, ondone: move |_| ask_wifi.set(false) }
        }
    }
}

/// A vertical toggle tile: icon well, title, what it is now.
#[component]
fn Toggle(
    icon: &'static str,
    title: &'static str,
    on: bool,
    sub: String,
    #[props(default)] disabled: bool,
    onflip: EventHandler<()>,
    #[props(default)] onmore: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        button {
            class: if on { "qt on" } else { "qt" },
            disabled,
            "aria-pressed": "{on}",
            onclick: move |_| onflip.call(()),
            oncontextmenu: move |e| if let Some(m) = onmore { e.prevent_default(); m.call(()); },
            span { class: "ic", span { key: "{on}", class: "glyph bump", Icon { name: icon } } }
            span { class: "qtl", b { "{title}" } small { "{sub}" } }
        }
    }
}
