//! Wi-Fi & Bluetooth on the computer.
//!
//! Wi-Fi is the one control here that can cut the connection it is being used over, so nothing
//! on it happens on a single tap: each change is asked first, says plainly what it risks, and
//! is put on probation by the daemon (`server::host::wifi`) — a network that does not work is
//! undone after a minute, and "off" can be for five minutes only.

use dioxus::prelude::*;
use wado_protocol::{host::Network, HostAction};

use crate::{
    state::Ui,
    ui::{
        host::act,
        widgets::{Icon, SwitchRow},
    },
};

#[derive(Clone, PartialEq)]
enum Ask {
    Off,
    Join(Network),
}

pub fn render(ui: Ui) -> Element {
    let mut ask = use_signal(|| None::<Ask>);
    let mut password = use_signal(String::new);
    let owner = (ui.live.host)().owner;
    let Some(h) = (ui.live.hoststate)() else {
        return rsx! { div { class: "card", p { class: "why", "Waiting for the computer…" } } };
    };

    rsx! {
        if let Some(secs) = h.wifi_revert_in {
            div { class: "card confirm",
                b { "Keep this Wi-Fi?" }
                p { class: "why", "The computer goes back to how it was in {secs} s unless you keep it." }
                button { class: "btn primary", onclick: move |_| act(HostAction::WifiKeep), "Keep" }
            }
        }

        if let Some(w) = h.wifi.clone() {
            div { class: "card",
                SwitchRow {
                    title: "Wi-Fi",
                    sub: match (&w.connected, w.enabled) {
                        (Some(n), _) => format!("Connected to {n}"),
                        (None, true) => "On, not connected".to_string(),
                        (None, false) => "Off".to_string(),
                    },
                    on: w.enabled,
                    disabled: !owner,
                    ontoggle: move |on: bool| if on { act(HostAction::WifiRadio { on: true, minutes: None }) } else { ask.set(Some(Ask::Off)) },
                }
                if !owner { p { class: "why", "Only the owner device can change the computer's Wi-Fi." } }
                if w.uplink { p { class: "why host", "This computer is online through Wi-Fi alone — the connection you are using right now goes through it." } }

                if ask() == Some(Ask::Off) {
                    div { class: "confirm",
                        b { "Turn off the computer's Wi-Fi?" }
                        p { if w.uplink {
                            "You will be disconnected straight away, and nothing here can turn it back on — choose a time limit and it comes back by itself."
                        } else {
                            "The computer has another connection, so you should stay connected."
                        } }
                        div { class: "actions",
                            button { class: "btn", onclick: move |_| ask.set(None), "Cancel" }
                            button { class: "btn primary", onclick: move |_| { act(HostAction::WifiRadio { on: false, minutes: Some(5) }); ask.set(None); }, "Off for 5 min" }
                        }
                        button { class: "btn danger", onclick: move |_| { act(HostAction::WifiRadio { on: false, minutes: None }); ask.set(None); },
                            if w.uplink { "Turn off until someone turns it on at the computer" } else { "Turn off" }
                        }
                    }
                }

                if w.enabled {
                    for n in w.networks.iter().cloned() {
                        {
                            let joining = ask() == Some(Ask::Join(n.clone()));
                            let n2 = n.clone();
                            rsx! {
                                button {
                                    key: "{n.ssid}",
                                    class: if n.active { "navrow netrow on" } else { "navrow netrow" },
                                    disabled: !owner || n.active,
                                    onclick: move |_| { password.set(String::new()); ask.set(Some(Ask::Join(n2.clone()))); },
                                    span { class: "disc", style: "--hue:var(--base0C)", Icon { name: "net" } }
                                    span { class: "navtext",
                                        b { "{n.ssid}" }
                                        small { "{bars(n.signal)}" if n.secure { " · secured" } if n.known { " · saved" } if n.active { " · connected" } }
                                    }
                                }
                                if joining {
                                    div { class: "confirm",
                                        b { "Join {n.ssid}?" }
                                        p { if w.uplink {
                                            "You will be disconnected while the computer switches. If it cannot get online through this network, it goes back to the one it was on after a minute."
                                        } else {
                                            "The computer switches its Wi-Fi to this network."
                                        } }
                                        if n.secure && !n.known {
                                            input { r#type: "password", placeholder: "Password", value: "{password}", oninput: move |e| password.set(e.value()) }
                                        }
                                        div { class: "actions",
                                            button { class: "btn", onclick: move |_| ask.set(None), "Cancel" }
                                            button {
                                                class: "btn primary",
                                                disabled: n.secure && !n.known && password().is_empty(),
                                                onclick: move |_| {
                                                    let pw = password();
                                                    act(HostAction::WifiConnect { ssid: n.ssid.clone(), password: (!pw.is_empty()).then_some(pw) });
                                                    ask.set(None);
                                                },
                                                "Join"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            div { class: "card", p { class: "why", "This computer has no NetworkManager, so its Wi-Fi cannot be changed from here." } }
        }

        if let Some(b) = h.bluetooth.clone() {
            div { class: "card",
                SwitchRow {
                    title: "Bluetooth",
                    sub: match b.devices.iter().filter(|d| d.connected).count() {
                        0 if b.powered => "On".to_string(),
                        0 => "Off".to_string(),
                        1 => format!("Connected to {}", b.devices.iter().find(|d| d.connected).map(|d| d.name.as_str()).unwrap_or("")),
                        n => format!("{n} devices connected"),
                    },
                    on: b.powered,
                    ontoggle: move |on: bool| act(HostAction::BtPower { on }),
                }
                if b.powered {
                    for d in b.devices.iter().cloned() {
                        div { key: "{d.addr}", class: "editrow",
                            span { class: "disc", style: "--hue:var(--base0D)", Icon { name: bt_icon(&d.icon) } }
                            b { "{d.name}" }
                            button {
                                class: if d.connected { "btn" } else { "btn primary" },
                                style: "width:auto;min-height:38px",
                                onclick: move |_| act(HostAction::BtConnect { addr: d.addr.clone(), connect: !d.connected }),
                                if d.connected { "Disconnect" } else { "Connect" }
                            }
                        }
                    }
                    p { class: "why", "Paired devices. Pair new ones in the computer's own settings." }
                }
            }
        }
    }
}

fn bars(signal: u8) -> &'static str {
    match signal {
        75.. => "Strong",
        50..=74 => "Good",
        25..=49 => "Fair",
        _ => "Weak",
    }
}

fn bt_icon(icon: &str) -> &'static str {
    match icon {
        i if i.starts_with("audio") => "sound",
        i if i.contains("gaming") => "pad",
        i if i.starts_with("input") => "kbd",
        _ => "bt",
    }
}
