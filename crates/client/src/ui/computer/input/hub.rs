//! The Virtual input hub (§3.2): the lead, the grant line, three device cards — each a live
//! miniature of itself (§5.1; tap it or Open to use it, the sliders button for its settings) —
//! what holds for all three, and the computer's own switches for every device (`settings`).

use dioxus::prelude::*;

use super::{
    grant_line::GrantLine,
    open::{self, Door},
    prefs::{self, GAMEPAD, TOUCHPAD},
};
use crate::{
    bridge, reach,
    state::Ui,
    ui::widgets::{Icon, NavRow, Seg, SwitchRow, When, WhenBadge},
};

pub fn render(ui: Ui) -> Element {
    let pk = crate::profile::current_key(ui);
    let r = reach::of(ui, &pk);
    let name = crate::profile::current_name(ui);
    let gp = GAMEPAD.read().clone();
    let tp = TOUCHPAD.read().clone();
    let cards = [
        (
            Door::Touchpad,
            "Touchpad",
            "Move the pointer, click, scroll".to_string(),
            "touchpad",
        ),
        (
            Door::Keyboard,
            "Keyboard",
            "Type with this phone's keyboard, voice too".to_string(),
            "keyboard",
        ),
        (
            Door::Gamepad,
            "Gamepad",
            format!("{} · a controller {name}'s games can see", gp.away_name()),
            "gamepad",
        ),
    ];
    let host = r.config.as_ref().and_then(|c| c.input_host.clone());
    let may_settings = r.may("settings");
    let session_pad = (ui.live.session_on)() && (ui.set.pad_on)();
    let haptics = if gp.haptics.is_empty() {
        "medium".to_string()
    } else {
        gp.haptics.clone()
    };

    rsx! {
        div { class: "vihub",
            p { class: "vilead", "Use this phone as {name}'s touchpad, keyboard or gamepad. Works with no session running." }
            GrantLine { r: r.clone(), name: name.clone() }
            div { class: "videvs",
                for (door, title, desc, vi) in cards {
                    {
                        let why = open::shut(ui, &r, door);
                        rsx! {
                            article { key: "{vi}", class: "videv", "aria-label": "{title}",
                                button {
                                    class: "vistage",
                                    "aria-label": "Open the {title.to_lowercase()}",
                                    disabled: why.is_some(),
                                    onclick: move |_| open::open(ui, door),
                                    div { class: "vimini", "data-vi": "mini-{vi}", "aria-hidden": "true" }
                                }
                                div { class: "virow",
                                    div { class: "viwords", b { "{title}" } small { if let Some(w) = &why { "{w}" } else { "{desc}" } } }
                                    button { class: "iconbtn vitune", "aria-label": "{title} settings", onclick: move |_| open::settings(ui, door),
                                        Icon { name: "sliders" }
                                    }
                                    button { class: "viopenpill", disabled: why.is_some(), onclick: move |_| open::open(ui, door), "Open" }
                                }
                            }
                        }
                    }
                }
            }
            if session_pad {
                p { class: "vinote", "The gamepad in your session and this one are two controllers to {name}: games number them as they appear." }
            }
            div { class: "sect", span { "On all three" } }
            div { class: "card vicard",
                div { class: "cardhead", "Feel in the hand" }
                Seg {
                    options: crate::ui::pages::opts(&[("off", "Off"), ("light", "Light"), ("medium", "Medium"), ("strong", "Strong")]),
                    value: haptics,
                    onpick: move |v: String| {
                        prefs::set_gamepad("haptics", &v);
                        let ms = match v.as_str() { "light" => 6, "medium" => 10, "strong" => 18, _ => 0 };
                        if ms > 0 { bridge::call(format!("try {{ navigator.vibrate && navigator.vibrate({ms}); }} catch (_) {{}}")); }
                    },
                }
                small { class: "vicap", "A tick on every click, key and button. Your phone's own vibration setting still wins." }
                SwitchRow {
                    title: "Left-handed",
                    sub: "Mirrors the gamepad: sticks and buttons swap sides",
                    on: gp.hand == "left",
                    ontoggle: move |v: bool| prefs::set_gamepad("hand", if v { "left" } else { "right" }),
                }
            }
            div { class: "list vilist",
                NavRow {
                    icon: "bolt", hue: "0D", title: "Quick actions",
                    sub: format!("{} chosen · beside the touchpad, in the widget and in a session", tp.get("quick").and_then(|q| q.as_array()).map_or(4, |a| a.len())),
                    onopen: move |_| bridge::call("window.__wado.padSettings.openHere(null, 'quick');".into()),
                }
            }
            if may_settings {
                if let Some(h) = host {
                    div { class: "sect", span { "For every device on {name}" } WhenBadge { when: When::Host } }
                    div { class: "card vicard vihost",
                        SwitchRow {
                            title: "Virtual input",
                            sub: "Touchpad, keyboard and gamepad, for every phone allowed input",
                            on: h.enabled,
                            ontoggle: move |v: bool| crate::ui::pages::host::set("trackpad.enabled", v.to_string(), true),
                        }
                        SwitchRow {
                            title: "Smoothing",
                            sub: "Plays touches out evenly, so a jittery network still feels like one hand",
                            on: h.smoothing,
                            disabled: !h.enabled,
                            ontoggle: move |v: bool| crate::ui::pages::host::set("trackpad.smoothing", v.to_string(), false),
                        }
                        div { class: "cardhead", "Report rate" }
                        Seg {
                            options: crate::ui::pages::opts(&[("60", "60 Hz"), ("125", "125 Hz"), ("250", "250 Hz")]),
                            value: h.rate_hz.to_string(),
                            disabled: !h.enabled,
                            onpick: move |v: String| crate::ui::pages::host::set("trackpad.rate-hz", v, false),
                        }
                    }
                }
            }
        }
    }
}
