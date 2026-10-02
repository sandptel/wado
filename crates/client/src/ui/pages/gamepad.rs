//! Gamepad: the on-screen pad — what it drives, its layout, size and see-through.

use dioxus::prelude::*;

use super::opts;
use crate::{
    bridge,
    state::Ui,
    ui::{
        gamepad::apply,
        widgets::{Info, Seg, SwitchRow},
    },
};

const PAD_INFO: &str = "Creates a virtual Xbox 360 controller on the host through /dev/uinput, so \
    every application on that machine sees a real pad — including games that read no keyboard at \
    all. Needs the daemon's user in the `uinput` group; if it is not, the log says so and nothing \
    moves.";
const KEYS_INFO: &str = "Maps the pad onto the keyboard and mouse events this session already \
    carries: left stick walks (WASD), right stick looks, A jumps, shoulders click the mouse. \
    Nothing is needed on the host — but a game that reads only a controller will not see this.";

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let mut live = ui.live;
    let mode = (s.pad_mode)();
    let on = (s.pad_on)();

    rsx! {
        div { class: "card",
            SwitchRow {
                title: "On-screen gamepad",
                sub: "Drawn over the picture",
                on,
                ontoggle: move |v| { s.pad_on.set(v); apply(ui); },
            }
            div { class: "cardhead", "The buttons drive"
                Info { text: if mode == "pad" { PAD_INFO } else { KEYS_INFO } }
            }
            Seg {
                options: opts(&[("keys", "Keys & mouse"), ("pad", "Real controller")]),
                value: mode.clone(),
                onpick: move |v| { s.pad_mode.set(v); apply(ui); },
            }
        }
        div { class: "card",
            SwitchRow {
                title: "Edit layout",
                sub: "Drag clusters; tap one to resize",
                on: (live.pad_edit)(),
                disabled: !on,
                ontoggle: move |v| { live.pad_edit.set(v); apply(ui); },
            }
            for (label, min, max, step, value, field) in [
                ("Size", 0.6, 1.8, 0.05, (s.pad_scale)(), 0),
                ("Opacity", 0.15, 1.0, 0.05, (s.pad_opacity)(), 1),
                ("Side inset", 0.0, 160.0, 4.0, (s.pad_inset_x)(), 2),
                ("Top/bottom inset", 0.0, 120.0, 4.0, (s.pad_inset_y)(), 3),
            ] {
                div { key: "{label}", class: "slide",
                    input {
                        r#type: "range", min: "{min}", max: "{max}", step: "{step}", "aria-label": "{label}",
                        value: "{value}",
                        oninput: move |e| if let Ok(v) = e.value().parse::<f64>() {
                            match field {
                                0 => s.pad_scale.set(v),
                                1 => s.pad_opacity.set(v),
                                2 => s.pad_inset_x.set(v),
                                _ => s.pad_inset_y.set(v),
                            }
                            apply(ui);
                        },
                    }
                    span { class: "slidelab", "{label}" }
                    span { class: "slideval",
                        if field == 1 { "{(value * 100.0).round()}%" }
                        else if field == 0 { "{value:.2}×" }
                        else { "{value:.0} px" }
                    }
                }
            }
            button {
                class: "btn",
                onclick: move |_| bridge::call("window.__wado.resetPadLayout();".to_string()),
                "Reset the layout"
            }
        }
    }
}
