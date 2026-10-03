//! The sound card on the control centre's root: where the computer's sound plays, and how loud
//! — on both ends. The Sound page has the per-app detail.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{
        host::{self, act},
        pages::Page,
        widgets::Icon,
    },
};
use wado_protocol::HostAction;

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let Some(h) = (live.hoststate)() else {
        return rsx! {};
    };
    let phone = host::on_phone(&h);
    let speaker = host::speaker(&h).cloned();
    let playing = h.audio.streams.len();
    // The phone first, then the computer's own outputs.
    let outputs: Vec<(String, String, bool)> = h
        .audio
        .phone_sink
        .iter()
        .map(|p| (p.clone(), "This phone".to_string(), true))
        .chain(
            h.audio
                .sinks
                .iter()
                .filter(|s| Some(&s.name) != h.audio.phone_sink.as_ref())
                .map(|s| (s.name.clone(), s.label.clone(), false)),
        )
        .collect();

    rsx! {
        div { class: "soundcard",
            div { class: "cardhead",
                "Sound"
                span { class: "why", if playing == 0 { " · nothing playing" } else if playing == 1 { " · 1 app playing" } else { " · {playing} apps playing" } }
                button { class: "linkbtn", onclick: move |_| live.cc_page.set(Page::Sound), "Apps & outputs" Icon { name: "right" } }
            }
            // Every output, one tap each: this phone, the computer's speakers, headphones, HDMI…
            div { class: "chips outs", role: "radiogroup", "aria-label": "Plays on",
                for (name, label, is_phone) in outputs {
                    button {
                        key: "{name}",
                        class: if name == h.audio.default_sink { "chip on" } else { "chip" },
                        role: "radio",
                        "aria-checked": "{name == h.audio.default_sink}",
                        onclick: move |_| {
                            host::act(HostAction::AllTo { sink: name.clone() });
                            // Sound on the phone needs a peer connection carrying it; off it, none.
                            crate::bridge::call(if is_phone {
                                "window.__wado.listenStart(); window.__wado.audioUnlock();".to_string()
                            } else {
                                "window.__wado.listenStop();".to_string()
                            });
                        },
                        Icon { name: if is_phone { "phone" } else { "monitor" } }
                        "{label}"
                    }
                }
            }
            if let Some(s) = speaker {
                div { class: "volrow",
                    button {
                        class: if s.muted { "iconbtn on" } else { "iconbtn" },
                        "aria-label": if s.muted { "Unmute the computer" } else { "Mute the computer" },
                        onclick: move |_| act(HostAction::SinkMute { id: s.id, muted: !s.muted }),
                        Icon { name: if s.muted { "mute" } else { "monitor" } }
                    }
                    div { class: "slide thin",
                        input {
                            r#type: "range", min: "0", max: "1", step: "0.02", "aria-label": "Computer volume",
                            value: "{s.volume}",
                            onchange: move |e| if let Ok(v) = e.value().parse::<f32>() { act(HostAction::SinkVolume { id: s.id, volume: v }) },
                        }
                        span { class: "slidelab", "Computer" }
                        span { class: "slideval", if s.muted { "muted" } else { "{(s.volume * 100.0).round()}%" } }
                    }
                }
            }
            if phone || (live.session_on)() {
                div { class: "volrow",
                    button {
                        class: if (ui.set.muted)() { "iconbtn on" } else { "iconbtn" },
                        "aria-label": "Mute this phone",
                        onclick: move |_| {
                            let mut s = ui.set;
                            s.muted.set(!(s.muted)());
                            crate::ui::live::apply(ui);
                            crate::bridge::call("window.__wado.audioUnlock();".to_string());
                        },
                        Icon { name: if (ui.set.muted)() { "mute" } else { "phone" } }
                    }
                    div { class: "slide thin",
                        input {
                            r#type: "range", min: "0", max: "1", step: "0.02", "aria-label": "Volume on this phone",
                            value: "{(ui.set.volume)()}",
                            oninput: move |e| if let Ok(v) = e.value().parse::<f64>() {
                                let mut s = ui.set;
                                s.volume.set(v);
                                if v > 0.0 { s.muted.set(false); }
                                crate::ui::live::apply(ui);
                            },
                        }
                        span { class: "slidelab", "This phone" }
                        span { class: "slideval", if (ui.set.muted)() { "muted" } else { "{((ui.set.volume)() * 100.0).round()}%" } }
                    }
                }
            }
        }
    }
}
