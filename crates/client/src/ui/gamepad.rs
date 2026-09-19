//! Gamepad group: the on-screen controller, and where it sits.
//!
//! Its own group rather than a corner of Live, because the mode choice needs room to explain
//! itself: the two modes are not a preference between equivalent things — one needs a host
//! permission most machines do not grant yet, and the other cannot drive a game that reads
//! only a controller. Someone who picks the wrong one gets a pad that does nothing, and the
//! only place to say why is next to the switch.
//!
//! Everything here applies immediately: the overlay is browser-side, so there is nothing to
//! restart and nothing the running session has to be told.

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

/// Push the whole pad config to the bridge.
///
/// The whole config, never one field: `js/gamepad.js` keeps a single object and re-applies it
/// wholesale, so a partial update is the one way the panel and the overlay could disagree.
/// Also called from [`crate::bridge::run`] after a reload, for the same reason
/// [`crate::ui::live::apply`] is — restoring the signals does not move the browser.
pub fn apply(ui: Ui) {
    let s = ui.set;
    bridge::call(format!(
        "window.__wado.setGamepad({{on:{},mode:{},scale:{},opacity:{},insetX:{},insetY:{}}});",
        (s.pad_on)(),
        bridge::js(&(s.pad_mode)()),
        (s.pad_scale)(),
        (s.pad_opacity)(),
        (s.pad_inset_x)(),
        (s.pad_inset_y)(),
    ));
}

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let mode = (s.pad_mode)();

    rsx! {
        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.pad_on)(),
                onchange: move |e| { s.pad_on.set(e.checked()); apply(ui); },
            }
            " Show the on-screen gamepad"
        }

        label { "What the buttons do" }
        select {
            value: "{mode}",
            onchange: move |e| { s.pad_mode.set(e.value()); apply(ui); },
            option { value: "keys", "Keys and mouse — inside this session" }
            option { value: "pad", "Real controller — everywhere on the machine" }
        }
        // The consequence of each, in the terms someone debugging a dead pad needs. Not a
        // tooltip: this is the setting people get wrong, and advice you have to hover for is
        // advice nobody reads.
        p { class: "hint",
            if mode == "pad" {
                "Creates a virtual Xbox 360 controller on the host through /dev/uinput, so
                 every application on that machine sees a real pad — including games that read
                 no keyboard at all. Needs the daemon's user to be in the `uinput` group; if it
                 is not, the log says so and nothing moves."
            } else {
                "Maps the pad onto the keyboard and mouse events this session already carries:
                 left stick walks (WASD), right stick looks, A jumps, shoulders click the
                 mouse. Nothing is needed on the host — but a game that reads only a controller
                 will not see this."
            }
        }

        label { "Size: {(s.pad_scale)():.2}×" }
        input {
            r#type: "range", min: "0.6", max: "1.8", step: "0.05",
            value: "{(s.pad_scale)()}",
            oninput: move |e| {
                if let Ok(v) = e.value().parse::<f64>() { s.pad_scale.set(v); apply(ui); }
            },
        }

        label { "Opacity: {((s.pad_opacity)() * 100.0).round()}%" }
        input {
            r#type: "range", min: "0.15", max: "1", step: "0.05",
            value: "{(s.pad_opacity)()}",
            oninput: move |e| {
                if let Ok(v) = e.value().parse::<f64>() { s.pad_opacity.set(v); apply(ui); }
            },
        }

        // Position, as distance from the edges rather than absolute coordinates: the two
        // halves are anchored to opposite corners, so one number moves both of them inward
        // and the layout stays symmetric on a screen of any shape.
        label { "Distance from the side edges: {(s.pad_inset_x)():.0} px" }
        input {
            r#type: "range", min: "0", max: "160", step: "4",
            value: "{(s.pad_inset_x)()}",
            oninput: move |e| {
                if let Ok(v) = e.value().parse::<f64>() { s.pad_inset_x.set(v); apply(ui); }
            },
        }
        label { "Distance from the top and bottom: {(s.pad_inset_y)():.0} px" }
        input {
            r#type: "range", min: "0", max: "120", step: "4",
            value: "{(s.pad_inset_y)()}",
            oninput: move |e| {
                if let Ok(v) = e.value().parse::<f64>() { s.pad_inset_y.set(v); apply(ui); }
            },
        }
    }
}
