//! Playback: what is playing on the computer, with its controls — on the landing page and at
//! the top of the control centre, Android-quick-settings style.
//!
//! The first player is the full card: its cover (as it is, unblurred), its app's own icon,
//! title and artist, play/pause, previous/next, a progress bar to drag, shuffle, repeat, its
//! volume, and where its sound comes out (*This phone* / *Computer*, a tap moves it). Every other
//! player gets a compact row with its own play/pause; tapping a row makes it the card.
//!
//! The bar runs on a CSS animation between polls — and polls quicken while something plays —
//! so it moves smoothly and stays in step with no timer of its own.

use dioxus::prelude::*;
use wado_protocol::{
    host::{MediaOp, Player},
    HostAction, HostState,
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

/// The app's icon if it has one; nothing otherwise — no stand-in glyph.
#[component]
fn AppIcon(p: Player) -> Element {
    match p.icon {
        Some(src) => rsx! { img { class: "mediaicon", src: "{src}", alt: "" } },
        None => rsx! {},
    }
}

pub fn render(ui: Ui) -> Element {
    let mut pick = use_signal(|| None::<String>);
    let Some(h) = (ui.live.hoststate)() else {
        return rsx! {};
    };
    if h.media.is_empty() {
        return rsx! {};
    }
    // The chosen player while it lasts, else the first (playing ones sort first).
    let p = pick()
        .and_then(|b| h.media.iter().find(|p| p.bus == b).cloned())
        .unwrap_or_else(|| h.media[0].clone());
    let others: Vec<Player> = h.media.iter().filter(|o| o.bus != p.bus).cloned().collect();

    rsx! {
        {card(&h, &p)}
        if !others.is_empty() {
            div { class: "mediarows",
                for o in others {
                    {
                        let (a, b) = (o.clone(), o.clone());
                        rsx! {
                            div { key: "{o.bus}", class: "mediarow",
                                button { class: "mediarowpick", onclick: move |_| pick.set(Some(a.bus.clone())),
                                    AppIcon { p: o.clone() }
                                    span { class: "navtext",
                                        b { if o.title.is_empty() { "{o.app}" } else { "{o.title}" } }
                                        small { if o.artist.is_empty() { "{o.app}" } else { "{o.artist} · {o.app}" } }
                                    }
                                }
                                button {
                                    class: "iconbtn",
                                    "aria-label": if o.playing { "Pause" } else { "Play" },
                                    onclick: move |_| op(&b, MediaOp::PlayPause),
                                    Icon { name: if o.playing { "pause" } else { "play" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn card(h: &HostState, p: &Player) -> Element {
    // Where it plays: the stream the player was matched to, else where everything goes.
    let stream = p
        .stream
        .and_then(|id| h.audio.streams.iter().find(|s| s.id == id))
        .cloned();
    let sink = stream
        .as_ref()
        .and_then(|s| s.sink.clone())
        .unwrap_or_else(|| h.audio.default_sink.clone());
    let on_phone = h.audio.phone_sink.as_deref() == Some(sink.as_str());
    let speaker = host::speaker(h).map(|s| s.name.clone());
    let phone = h.audio.phone_sink.clone();
    let pct = if p.length_ms > 0 {
        (p.position_ms as f64 / p.length_ms as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let left_s = p.length_ms.saturating_sub(p.position_ms) as f64 / 1000.0;
    let (p1, p2, p3, p4, p5, p6, p7) = (
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
        p.clone(),
    );
    let next_loop = match p.loop_status.as_deref() {
        Some("None") => "Playlist",
        Some("Playlist") => "Track",
        _ => "None",
    };

    rsx! {
        div { class: "mediacard",
            div { class: "mediatop",
                span { class: "mediaapp", AppIcon { p: p.clone() } "{p.app}" }
                if let (Some(st), Some(phone)) = (stream.clone(), phone) {
                    button {
                        class: "mediaout",
                        title: "Where it plays — tap to move it",
                        onclick: move |_| {
                            let to = if on_phone { speaker.clone() } else { Some(phone.clone()) };
                            if let Some(sink) = to {
                                if !on_phone {
                                    crate::bridge::call("window.__wado.listenStart(); window.__wado.audioUnlock();".to_string());
                                }
                                host::act(HostAction::StreamTo { id: st.id, sink });
                            }
                        },
                        Icon { name: if on_phone { "phone" } else { "monitor" } }
                        if on_phone { "This phone" } else { "Computer" }
                    }
                }
            }
            div { class: "mediamid",
                if let Some(art) = p.art.clone() {
                    img { class: "mediaart", src: "{art}", alt: "" }
                }
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
            if p.length_ms > 0 {
                div { class: "mediabar",
                    // Keyed on the position, so each poll restarts the run from where it really is.
                    div {
                        key: "{p.bus}-{p.position_ms}-{p.playing}",
                        class: if p.playing { "mediafill run" } else { "mediafill" },
                        style: "--from:{pct:.2}%;--left:{left_s:.1}s",
                    }
                    if p.can_seek {
                        input {
                            r#type: "range", min: "0", max: "{p.length_ms}", step: "1000",
                            "aria-label": "Position", value: "{p.position_ms}",
                            onchange: move |e| if let Ok(ms) = e.value().parse::<u64>() { op(&p2, MediaOp::SeekTo { ms }) },
                        }
                    }
                }
                div { class: "mediatimes", span { "{clock(p.position_ms)}" } span { "{clock(p.length_ms)}" } }
            }
            div { class: "mediactl",
                if let Some(sh) = p.shuffle {
                    button { class: if sh { "on" } else { "" }, "aria-label": "Shuffle", "aria-pressed": "{sh}",
                        onclick: move |_| op(&p5, MediaOp::Shuffle { on: !sh }), Icon { name: "shuffle" } }
                }
                button { "aria-label": "Previous", disabled: !p.can_prev, onclick: move |_| op(&p3, MediaOp::Previous), Icon { name: "prev" } }
                button { "aria-label": "Next", disabled: !p.can_next, onclick: move |_| op(&p4, MediaOp::Next), Icon { name: "next" } }
                if let Some(l) = p.loop_status.clone() {
                    button {
                        class: if l == "None" { "" } else { "on" },
                        "aria-label": "Repeat: {l}",
                        title: "Repeat: {l}",
                        onclick: move |_| op(&p6, MediaOp::Loop { mode: next_loop.into() }),
                        Icon { name: if l == "Track" { "repeat1" } else { "repeat" } }
                    }
                }
                if let Some(st) = stream {
                    div { class: "slide thin mediavol",
                        input {
                            r#type: "range", min: "0", max: "1", step: "0.02", "aria-label": "Volume of {p7.app}",
                            value: "{st.volume}",
                            onchange: move |e| if let Ok(v) = e.value().parse::<f32>() { host::act(HostAction::StreamVolume { id: st.id, volume: v }) },
                        }
                        span { class: "slidelab", Icon { name: "sound" } }
                    }
                }
            }
        }
    }
}
