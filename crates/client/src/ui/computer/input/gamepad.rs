//! Gamepad settings (§5.4): the pad itself, live and pressable, with what the computer would see;
//! the layouts (a carousel of the real renderer at thumbnail size — tapping one morphs the
//! preview); Edit layout and Comfort check; which layout to use away from a session; what the
//! buttons drive; and the sticks. One page, two hosts: the Computer page and the in-session
//! control centre.

use dioxus::prelude::*;

use super::{
    open,
    prefs::{self, GAMEPAD},
};
use crate::{
    bridge,
    state::Ui,
    ui::widgets::{Icon, Seg, Slider},
};

const DRIVES_PAD: &str =
    "The computer sees an Xbox 360 controller, so games that read a pad work as they are.";
const DRIVES_KEYS: &str = "Each button presses a key or clicks the mouse in your session: the left stick walks with WASD, the right stick looks. Away from a session this is not available yet.";

pub fn render(ui: Ui) -> Element {
    let name = crate::profile::current_name(ui);
    let gp = GAMEPAD.read().clone();
    let mut more = use_signal(|| false);
    let away_own = !gp.away.is_empty() && gp.away != "same";
    let drives = if gp.drives.is_empty() {
        "pad".to_string()
    } else {
        gp.drives.clone()
    };
    let stick = if gp.stick.is_empty() {
        "fixed".to_string()
    } else {
        gp.stick.clone()
    };
    let away_seg = if away_own { "own" } else { "same" }.to_string();
    let stick_words = match stick.as_str() {
        "floating" => "The stick comes to where your thumb lands. Best when you never look down.",
        "relative" => {
            "Zero is wherever your thumb lands, and nothing moves on screen. Best for aiming."
        }
        _ => "The stick stays where it is drawn. Best when you look at the screen, not the phone.",
    };
    rsx! {
        div { class: "vipage",
            div { class: "vipreview",
                div { class: "vicap", span { "Try it · press anything" } span { class: "videmo mono", "data-vi-demo": "gamepad", "—" } }
                div { class: "vigpv", "data-vi": "gamepad", "aria-hidden": "true" }
            }
            div { class: "sect", span { if away_own { "Layout in a session" } else { "Layout" } } }
            div { class: "vicarousel", "data-vi": "gp-layouts", "data-where": "session", role: "radiogroup", "aria-label": "Layout" }
            div { class: "card vicard",
                div { class: "virow2",
                    button { class: "btn", onclick: move |_| open::edit(ui, "edit"), Icon { name: "edit" } "Edit layout" }
                    button { class: "btn", onclick: move |_| open::edit(ui, "comfort"), Icon { name: "hand" } "Comfort check" }
                }
                div { class: "cardhead", "Away from a session, use" }
                Seg {
                    options: crate::ui::pages::opts(&[("same", "Same as in a session"), ("own", "Its own layout")]),
                    value: away_seg.clone(),
                    onpick: move |v: String| {
                        let g = GAMEPAD.peek().clone();
                        prefs::set_gamepad("away", if v == "same" { "same" } else { &g.layout });
                    },
                }
                small { class: "vicap", "“Same” keeps one set of controls everywhere: what you arrange for a game's session is what you get on the couch." }
            }
            if away_own {
                div { class: "sect", span { "Layout away from a session" } }
                div { class: "vicarousel", "data-vi": "gp-layouts", "data-where": "away", role: "radiogroup", "aria-label": "Layout away from a session" }
            }
            div { class: "sect", span { "The buttons drive" } }
            div { class: "card vicard",
                Seg {
                    options: crate::ui::pages::opts(&[("pad", "A real controller"), ("keys", "Keys & mouse")]),
                    value: drives.clone(),
                    onpick: move |v: String| prefs::set_gamepad("drives", &v),
                }
                p { class: "viinfo", span { if drives == "keys" { "{DRIVES_KEYS}" } else { "{DRIVES_PAD}" } } }
                button { class: "vimore", "aria-expanded": "{more()}", onclick: move |_| more.toggle(),
                    if more() { "Less" } else { "More about this" }
                }
                if more() {
                    p { class: "videtail",
                        "The controller is a real device on {name} ("
                        span { class: "mono", "/dev/uinput" } ", " span { class: "mono", "045e:028e" }
                        "), made when you open the gamepad and removed 30 seconds after you leave it, so a game does not see it unplugged while you glance at the touchpad. Away from a session it is the only choice for now."
                    }
                }
            }
            div { class: "sect", span { "Sticks" } }
            div { class: "card vicard",
                Seg {
                    options: crate::ui::pages::opts(&[("fixed", "Fixed"), ("floating", "Floating"), ("relative", "Relative")]),
                    value: stick,
                    onpick: move |v: String| bridge::call(format!("window.__wado.inputPreview.gamepadSet('stick', {});", bridge::js(&v))),
                }
                p { class: "viinfo", span { "{stick_words}" } }
                Slider {
                    label: "Dead zone",
                    value: gp.dead,
                    min: 0.0,
                    max: 0.4,
                    step: 0.01,
                    shown: format!("{}%", (gp.dead * 100.0).round()),
                    oninput: move |v: f64| bridge::call(format!("window.__wado.inputPreview.gamepadSet('dead', {v});")),
                }
                small { class: "vicap", "How far a thumb moves before the stick counts. Shown on the sticks above while you change it." }
            }
        }
    }
}
