//! Sound — the computer's mixer, pavucontrol-style: everything playing, each with its own
//! volume, mute and output; and every output, with its volume and which one is the default.
//! Works with no session: only the relay link is needed.

use dioxus::prelude::*;
use wado_protocol::HostAction;

use crate::{
    state::Ui,
    ui::{host::act, widgets::Icon},
};

/// A sink label short enough for a chip.
fn short(label: &str) -> String {
    let l = label
        .replace("High Definition Audio Controller", "")
        .replace("HD Audio Controller", "");
    let l = l.trim();
    if l.chars().count() > 22 {
        l.chars().take(21).collect::<String>() + "…"
    } else {
        l.to_string()
    }
}

pub fn render(ui: Ui) -> Element {
    let Some(h) = (ui.live.hoststate)() else {
        return rsx! { div { class: "card", p { class: "why", "Waiting for the computer…" } } };
    };
    let a = h.audio.clone();
    let chips: Vec<(String, String)> = a
        .sinks
        .iter()
        .map(|s| {
            let label = if a.phone_sink.as_deref() == Some(s.name.as_str()) {
                crate::ui::host::here(ui)
            } else {
                short(&s.label)
            };
            (s.name.clone(), label)
        })
        .collect();

    rsx! {
        div { class: "sect", span { "Playing now" } }
        if a.streams.is_empty() {
            div { class: "card", p { class: "why", "Nothing is playing on the computer." } }
        }
        for st in a.streams.iter().cloned() {
            div { key: "{st.id}", class: "card",
                div { class: "streamhead",
                    span { class: "disc", style: "--hue:var(--base0E)", Icon { name: "sound" } }
                    div { class: "navtext",
                        b { "{st.app}" }
                        if !st.title.is_empty() && st.title != st.app { small { "{st.title}" } }
                    }
                }
                div { class: "volrow",
                    button {
                        class: if st.muted { "iconbtn on" } else { "iconbtn" },
                        "aria-label": if st.muted { "Unmute" } else { "Mute" },
                        onclick: move |_| act(HostAction::StreamMute { id: st.id, muted: !st.muted }),
                        Icon { name: if st.muted { "mute" } else { "sound" } }
                    }
                    div { class: "slide thin",
                        input {
                            r#type: "range", min: "0", max: "1", step: "0.02", "aria-label": "Volume of {st.app}",
                            value: "{st.volume}",
                            onchange: move |e| if let Ok(v) = e.value().parse::<f32>() { act(HostAction::StreamVolume { id: st.id, volume: v }) },
                        }
                        span { class: "slideval", "{(st.volume * 100.0).round()}%" }
                    }
                }
                if chips.len() > 1 {
                    // Chips that wrap, not a segmented row: a computer can have half a dozen
                    // outputs, and every name has to stay readable.
                    div { class: "chips", role: "radiogroup", "aria-label": "Plays on",
                        for (name, label) in chips.iter().cloned() {
                            button {
                                key: "{name}",
                                class: if st.sink.as_deref() == Some(name.as_str()) { "chip on" } else { "chip" },
                                role: "radio",
                                "aria-checked": "{st.sink.as_deref() == Some(name.as_str())}",
                                onclick: move |_| {
                                    let to_phone = (ui.live.hoststate)().is_some_and(|h| h.audio.phone_sink.as_ref() == Some(&name));
                                    if to_phone {
                                        crate::bridge::call("window.__wado.listenStart(); window.__wado.audioUnlock();".to_string());
                                    }
                                    act(HostAction::StreamTo { id: st.id, sink: name.clone() })
                                },
                                Icon { name: if a.phone_sink.as_deref() == Some(name.as_str()) { "phone" } else { "monitor" } }
                                "{label}"
                            }
                        }
                    }
                }
            }
        }

        div { class: "sect", span { "Outputs" } }
        for s in h.audio.sinks.iter().cloned() {
            div { key: "{s.id}", class: "card",
                div { class: "streamhead",
                    span { class: "disc", style: "--hue:var(--base0D)", Icon { name: if Some(&s.name) == h.audio.phone_sink.as_ref() { "phone" } else { "monitor" } } }
                    div { class: "navtext", b { "{s.label}" } }
                    if s.name == h.audio.default_sink {
                        span { class: "whenbadge apply", "default" }
                    } else {
                        button { class: "linkbtn", onclick: move |_| act(HostAction::DefaultSink { name: s.name.clone() }), "Make default" }
                    }
                }
                div { class: "volrow",
                    button {
                        class: if s.muted { "iconbtn on" } else { "iconbtn" },
                        "aria-label": if s.muted { "Unmute" } else { "Mute" },
                        onclick: move |_| act(HostAction::SinkMute { id: s.id, muted: !s.muted }),
                        Icon { name: if s.muted { "mute" } else { "sound" } }
                    }
                    div { class: "slide thin",
                        input {
                            r#type: "range", min: "0", max: "1", step: "0.02", "aria-label": "Volume of {s.label}",
                            value: "{s.volume}",
                            onchange: move |e| if let Ok(v) = e.value().parse::<f32>() { act(HostAction::SinkVolume { id: s.id, volume: v }) },
                        }
                        span { class: "slideval", "{(s.volume * 100.0).round()}%" }
                    }
                }
            }
        }
        p { class: "why", "\"This device\" plays on the device you are holding. Moving an app there sends its sound here instead of out of the computer." }
    }
}
