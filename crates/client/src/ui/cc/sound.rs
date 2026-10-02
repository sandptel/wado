//! The sound card on the control centre's root: where the computer's sound plays, and how loud
//! — on both ends. The Sound page has the per-app detail.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    ui::{
        host::{self, act},
        pages::Page,
        widgets::{Icon, Seg},
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
    let h2 = h.clone();
    let h3 = h.clone();

    rsx! {
        div { class: "soundcard",
            div { class: "cardhead",
                "Sound"
                span { class: "why", if playing == 0 { " · nothing playing" } else if playing == 1 { " · 1 app playing" } else { " · {playing} apps playing" } }
                button { class: "linkbtn", onclick: move |_| live.cc_page.set(Page::Sound), "Apps & outputs" Icon { name: "right" } }
            }
            if h.audio.phone_sink.is_some() {
                Seg {
                    options: vec![("computer".to_string(), "On the computer".to_string()), ("phone".to_string(), "On this phone".to_string())],
                    value: if phone { "phone" } else { "computer" },
                    onpick: move |v: String| if v == "phone" { host::play_on_phone(&h2) } else { host::play_on_computer(&h3) },
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
