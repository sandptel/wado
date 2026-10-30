//! Touchpad settings (§5.2): a sticky live preview that acts out each setting — slide to move
//! (pointer travel = finger travel × speed), two fingers scroll, tap to click, edge swipes and
//! holds running their action — then the settings, the phone gestures, and what the computer
//! decides itself.
//!
//! The settings are one table ([`SECTIONS`]) over one model (the trackpad's prefs as JSON,
//! `prefs::TOUCHPAD`), each drawn by [`super::setting::Setting`] and each naming the demo its
//! change runs. A new knob (a tap threshold, a gesture's distance) is one line here and one field
//! in `device { trackpad { } }`.
//!
//! One implementation of the trackpad's settings (`js/pad_settings.js`): its gesture pickers are
//! drawn here by it (`data-vi="tp-gestures"`), and inside the pad its ⚙ still opens it as a sheet.

use dioxus::prelude::*;

use super::{
    prefs::{self, TOUCHPAD},
    setting::{Kind, Setting, Spec, Unit},
};
use crate::{
    state::Ui,
    ui::widgets::{Icon, When, WhenBadge},
};

const HOLD: Spec = Spec {
    key: "landscape",
    title: "How you hold it",
    sub: "",
    kind: Kind::Pictures(&[("true", "Sideways", "side"), ("false", "Upright", "")]),
    demo: "glide",
    needs: None,
};

/// `(section, badge, settings)`. The badge says whose setting it is when it is not obvious.
pub const SECTIONS: &[(&str, &str, &[Spec])] = &[
    ("How you hold it", "", &[
        HOLD,
        Spec { key: "fullscreen", title: "Turn the screen for real", sub: "Goes full screen and locks the rotation, where the browser allows it",
            kind: Kind::Switch, demo: "", needs: Some("landscape") },
    ]),
    ("Beside the pad", "", &[
        Spec { key: "rail", title: "", sub: "", kind: Kind::Choice(&[("\"keys\"", "Keys"), ("\"actions\"", "Quick actions"), ("\"none\"", "Nothing")]),
            demo: "glide", needs: None },
        Spec { key: "keyboard", title: "Open with the keyboard up", sub: "The pad shrinks to a strip above it", kind: Kind::Switch, demo: "", needs: None },
    ]),
    ("Pointer", "this phone", &[
        Spec { key: "speed", title: "Speed",
            sub: "How far the pointer travels for the same slide. The computer still adds its own acceleration; a new speed takes hold the next time the touchpad opens.",
            kind: Kind::Slider { min: 0.5, max: 2.0, step: 0.05, unit: Unit::Times }, demo: "glide", needs: None },
    ]),
];

pub fn render(ui: Ui) -> Element {
    let name = crate::profile::current_name(ui);
    let model = TOUCHPAD.read().clone();
    let mut more = use_signal(|| false);
    let r = crate::reach::of(ui, &crate::profile::current_key(ui));
    let why = super::open::shut(ui, &r, super::open::Door::Touchpad);
    let set = move |(spec, v): (Spec, serde_json::Value)| {
        prefs::set_touchpad(spec.key, v);
        if !spec.demo.is_empty() {
            prefs::demo("touchpad", spec.demo);
        }
    };
    rsx! {
        div { class: "vipage",
            if !(ui.live.session_on)() {
                div { class: "vihead",
                    super::HeadOpen { label: "Open", disabled: why.is_some(), onopen: move |_| super::open::open(ui, super::open::Door::Touchpad) }
                    if let Some(w) = &why { small { class: "vicap", "{w}" } }
                }
            }
            div { class: "vipreview", "aria-hidden": "true",
                div { class: "vicap", span { "Preview" } span { class: "videmo", "data-vi-demo": "touchpad", "slide to move" } }
                div { class: "vitpv", "data-vi": "touchpad" }
            }
            for (title, badge, specs) in SECTIONS.iter().copied() {
                section { key: "{title}", class: "visection",
                    div { class: "sect", span { "{title}" } if !badge.is_empty() { span { class: "whenbadge apply", "{badge}" } } }
                    div { class: "card vicard",
                        for spec in specs.iter().copied() {
                            Setting { key: "{spec.key}", spec, model: model.clone(), onset: set }
                        }
                    }
                }
            }
            div { class: "sect", span { "Phone gestures" } }
            div { class: "card vicard",
                p { class: "viinfo", Icon { name: "info" }
                    span { "These belong to wado, never to {name}'s own touchpad gestures, so a three-finger swipe always reaches the desktop. All are off until you give them an action." }
                }
                div { class: "vigest", "data-vi": "tp-gestures" }
            }
            div { class: "sect", span { "Decided by {name}" } WhenBadge { when: When::Host } }
            div { class: "card vicard",
                p { class: "viinfo", Icon { name: "laptop" }
                    span { "Tap to click, scroll direction and acceleration are {name}'s own touchpad settings. wado's pad is an ordinary touchpad to it." }
                }
                button { class: "vimore", "aria-expanded": "{more()}", onclick: move |_| more.toggle(),
                    if more() { "Less" } else { "Where to change them" }
                }
                if more() {
                    p { class: "videtail",
                        "In Hyprland: " span { class: "mono", "input {{ touchpad {{ tap-to-click, natural_scroll }} }}" }
                        " in " span { class: "mono", "hyprland.conf" } ". The device is named "
                        span { class: "mono", "wado trackpad" } " in " span { class: "mono", "hyprctl devices" }
                        ", so it can have settings of its own."
                    }
                }
            }
        }
    }
}
