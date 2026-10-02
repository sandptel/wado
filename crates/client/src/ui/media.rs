//! The playback card: what is playing on the computer, with its controls — on the landing page
//! and at the top of the control centre, Android-quick-settings style.
//!
//! Its output chip says where the sound is coming out — *This phone* or the computer — and a
//! tap moves it. The progress bar runs on a CSS animation between polls, so it moves smoothly
//! without a timer, and dragging it seeks.

use dioxus::prelude::*;
use wado_protocol::{
    host::{MediaOp, Player},
    HostAction,
};

use crate::{
    state::Ui,
    ui::{host, widgets::Icon},
};

fn op(p: &Player, op: MediaOp) {
    host::act(HostAction::Media {
        bus: p.bus.clone(),
        op,
    });
}

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn render(ui: Ui) -> Element {
    let mut pick = use_signal(|| 0usize);
    let Some(h) = (ui.live.hoststate)() else {
        return rsx! {};
    };
    if h.media.is_empty() {
        return rsx! {};
    }
    let n = h.media.len();
    let p = h.media[pick().min(n - 1)].clone();
    // Where it plays: the stream the player was matched to, else where everything goes.
    let sink = p
        .stream
        .and_then(|id| h.audio.streams.iter().find(|s| s.id == id))
        .and_then(|s| s.sink.clone())
        .unwrap_or_else(|| h.audio.default_sink.clone());
    let on_phone = h.audio.phone_sink.as_deref() == Some(sink.as_str());
    let speaker = host::speaker(&h).map(|s| s.name.clone());
    let pct = if p.length_ms > 0 {
        (p.position_ms as f64 / p.length_ms as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let left_s = p.length_ms.saturating_sub(p.position_ms) as f64 / 1000.0;
    let bg = p
        .art
        .clone()
        .map(|a| format!("--art:url(\"{}\")", a.replace('"', "%22")))
        .unwrap_or_default();
    let (p1, p2, p3, p4, p5, p6) = (
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
    );
    let phone = h.audio.phone_sink.clone();

    rsx! {
        div { class: if p.art.is_some() { "mediacard art" } else { "mediacard" }, style: "{bg}",
            div { class: "mediatop",
                span { class: "mediaapp", Icon { name: "music" } "{p.app}" }
                if n > 1 {
                    button { class: "mediapick", "aria-label": "Next player", onclick: move |_| pick.set((pick() + 1) % n),
                        "{pick().min(n - 1) + 1}/{n}"
                    }
                }
                if let (Some(stream), Some(phone)) = (p.stream, phone) {
                    button {
                        class: "mediaout",
                        title: "Where it plays — tap to move it",
                        onclick: move |_| {
                            let to = if on_phone { speaker.clone() } else { Some(phone.clone()) };
                            if let Some(sink) = to {
                                if !on_phone {
                                    crate::bridge::call("window.__wado.listenStart(); window.__wado.audioUnlock();".to_string());
                                }
                                host::act(HostAction::StreamTo { id: stream, sink });
                            }
                        },
                        Icon { name: if on_phone { "phone" } else { "monitor" } }
                        if on_phone { "This phone" } else { "Computer" }
                    }
                }
            }
            div { class: "mediamid",
                div { class: "mediatext",
                    b { if p.title.is_empty() { "{p.app}" } else { "{p.title}" } }
                    if !p.artist.is_empty() { span { "{p.artist}" } }
                }
                button {
                    class: "mediaplay",
                    "aria-label": if p.playing { "Pause" } else { "Play" },
                    onclick: move |_| op(&p1, MediaOp::PlayPause),
                    Icon { name: if p.playing { "pause" } else { "play" } }
                }
            }
            div { class: "mediactl",
                button { "aria-label": "Previous", disabled: !p.can_prev, onclick: move |_| op(&p2, MediaOp::Previous), Icon { name: "prev" } }
                div { class: "mediabar",
                    // Keyed on the position, so each poll restarts the run from where it really is.
                    div {
                        key: "{p.bus}-{p.position_ms}-{p.playing}",
                        class: if p.playing && p.length_ms > 0 { "mediafill run" } else { "mediafill" },
                        style: "--from:{pct:.2}%;--left:{left_s:.1}s",
                    }
                    if p.can_seek {
                        input {
                            r#type: "range", min: "0", max: "{p.length_ms}", step: "1000",
                            "aria-label": "Position", value: "{p.position_ms}",
                            onchange: move |e| if let Ok(ms) = e.value().parse::<u64>() { op(&p3, MediaOp::SeekTo { ms }) },
                        }
                    }
                    if p.length_ms > 0 {
                        span { class: "mediatime", "{clock(p.position_ms)} / {clock(p.length_ms)}" }
                    }
                }
                button { "aria-label": "Next", disabled: !p.can_next, onclick: move |_| op(&p4, MediaOp::Next), Icon { name: "next" } }
                if let Some(sh) = p5.shuffle {
                    button {
                        class: if sh { "on" } else { "" },
                        "aria-label": "Shuffle",
                        "aria-pressed": "{sh}",
                        onclick: move |_| op(&p6, MediaOp::Shuffle { on: !sh }),
                        Icon { name: "shuffle" }
                    }
                }
            }
        }
    }
}
