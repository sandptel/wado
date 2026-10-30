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

/// A spec with the rest defaulted: no demo, always enabled, always available.
const fn spec(key: &'static str, title: &'static str, sub: &'static str, kind: Kind) -> Spec {
    Spec {
        key,
        title,
        sub,
        kind,
        demo: "",
        needs: None,
        cap: None,
    }
}
const fn on_host(mut s: Spec) -> Spec {
    s.cap = Some(HYPR);
    s
}
const fn demo(mut s: Spec, d: &'static str) -> Spec {
    s.demo = d;
    s
}
const fn needs(mut s: Spec, k: &'static str) -> Spec {
    s.needs = Some(k);
    s
}

/// What Hyprland lets the daemon set for this phone's touchpad alone (research §5).
const HYPR: &str = "pad-hypr";

/// `(section, badge, settings)`. Pointer, scrolling and tapping are libinput's, applied by the
/// computer to this phone's touchpad only (never the laptop's own pad, never a config file): the
/// research's two sliders and its switches (`docs/superpowers/research/2026-10-30-touchpad-feel.md`
/// §5, §9). The click bar and thumb marking are stored now and drawn when their cap arrives.
pub const SECTIONS: &[(&str, &str, &[Spec])] = &[
    ("How you hold it", "", &[
        demo(spec("landscape", "How you hold it", "", Kind::Pictures(&[("true", "Sideways", "side"), ("false", "Upright", "")])), "glide"),
        needs(spec("fullscreen", "Turn the screen for real", "Goes full screen and locks the rotation, where the browser allows it", Kind::Switch), "landscape"),
    ]),
    ("Beside the pad", "", &[
        demo(spec("rail", "", "", Kind::Choice(&[("\"keys\"", "Keys"), ("\"actions\"", "Quick actions"), ("\"none\"", "Nothing")])), "glide"),
        spec("keyboard", "Open with the keyboard up", "The pad shrinks to a strip above it", Kind::Switch),
    ]),
    ("Pointer and scrolling", "set by the computer", &[
        on_host(demo(spec("speed", "Pointer speed", "Changes only this phone's touchpad on the computer, which still accelerates faster slides.",
            Kind::Slider { min: 0.5, max: 2.0, step: 0.05, unit: Unit::Times }), "glide")),
        on_host(demo(spec("scroll_speed", "Scroll speed", "",
            Kind::Slider { min: 0.25, max: 2.0, step: 0.05, unit: Unit::Times }), "scroll")),
        on_host(demo(spec("natural", "Natural scrolling", "Content follows your fingers, as on a phone", Kind::Switch), "scroll")),
    ]),
    ("Tapping", "set by the computer", &[
        on_host(demo(spec("tap", "Tap to click", "One finger taps a left click, two a right click", Kind::Switch), "tap")),
        on_host(spec("tap_drag", "Tap, then drag", "Tap and put the finger back down to drag", Kind::Switch)),
        on_host(needs(spec("drag_lock", "Keep holding after you lift", "", Kind::Choice(&[("\"off\"", "Off"), ("\"briefly\"", "Briefly"), ("\"until-tap\"", "Until you tap")])), "tap_drag")),
        Spec { cap: Some("pad-click"), ..spec("click_bar", "Click bar", "A bar under the pad that clicks, for when taps do not", Kind::Switch) },
    ]),
    ("Fingers", "", &[
        Spec { cap: Some("pad-palm"), ..spec("palm", "Ignore resting thumbs", "A thumb resting on the edge is not a finger", Kind::Switch) },
        spec("marks", "Finger marks", "A ring under each finger on this phone", Kind::Switch),
    ]),
    ("Phone gestures", "", &[
        spec("gesture_distance", "How far a gesture goes", "wado's own edge swipes and holds only. The computer's three- and four-finger gestures are its own.",
            Kind::Choice(&[("\"short\"", "Short"), ("\"medium\"", "Medium"), ("\"long\"", "Long")])),
    ]),
];

pub fn render(ui: Ui) -> Element {
    let name = crate::profile::current_name(ui);
    let model = TOUCHPAD.read().clone();
    let caps = crate::reach::of(ui, &crate::profile::current_key(ui))
        .config
        .map(|c| c.caps)
        .unwrap_or_default();
    let mut more = use_signal(|| false);
    let set = move |(spec, v): (Spec, serde_json::Value)| {
        prefs::set_touchpad(spec.key, v);
        if !spec.demo.is_empty() {
            prefs::demo("touchpad", spec.demo);
        }
    };
    rsx! {
        div { class: "vipage",
            div { class: "vipreview", "aria-hidden": "true",
                div { class: "vicap", span { "Preview" } span { class: "videmo", "data-vi-demo": "touchpad", "slide to move" } }
                div { class: "vitpv", "data-vi": "touchpad" }
            }
            for (title, badge, specs) in SECTIONS.iter().copied() {
                {
                    let shown: Vec<Spec> = specs.iter().copied().filter(|s| s.cap.is_none_or(|c| caps.iter().any(|x| x == c))).collect();
                    let missing = specs.iter().any(|s| s.cap == Some("pad-hypr")) && !caps.iter().any(|x| x == "pad-hypr");
                    let badge = if badge == "set by the computer" { format!("set by {name}") } else { badge.to_string() };
                    rsx! {
                        section { key: "{title}", class: "visection",
                            div { class: "sect", span { "{title}" } if !badge.is_empty() && !missing { span { class: "whenbadge host", "{badge}" } } }
                            div { class: "card vicard",
                                for spec in shown {
                                    Setting { key: "{spec.key}", spec, model: model.clone(), onset: set }
                                }
                                if missing {
                                    p { class: "viinfo", Icon { name: "laptop" }
                                        span { "{name}'s desktop sets these for every touchpad. wado can set them for this phone alone only on Hyprland." }
                                    }
                                }
                                if title == "Phone gestures" {
                                    p { class: "viinfo", Icon { name: "info" }
                                        span { "These belong to wado, never to {name}'s own touchpad gestures, so a three-finger swipe always reaches the desktop. All are off until you give them an action." }
                                    }
                                    div { class: "vigest", "data-vi": "tp-gestures" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "sect", span { "Decided by {name}" } WhenBadge { when: When::Host } }
            div { class: "card vicard",
                p { class: "viinfo", Icon { name: "laptop" }
                    span { "How long a tap may last, how far it may move, and the three- and four-finger gestures are {name}'s, the same as for its own touchpad. wado's pad is an ordinary touchpad to it." }
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
