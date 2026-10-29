//! Now playing on a computer: the cover crisp on the accent wash (no forced white, so it reads
//! on light schemes), the app, title and artist, a scrubber, where it plays, and the controls —
//! with every other player as a compact row under it. On the Computer page and at the top of
//! the control centre: one component, two hosts.
//!
//! Motion, all transform/opacity: a new cover fades in over the old from 1.08 (the old one stays
//! under it until it is covered); a new title rises in; play/pause swaps its glyph with a pop;
//! the progress bar is a `scale` run `linear` between polls — it is a clock — re-keyed on every
//! poll so it restarts from where the player really is.

use dioxus::prelude::*;
use wado_protocol::{
    host::{MediaOp, Player},
    HostAction, HostState,
};

use crate::{
    route,
    state::Ui,
    ui::{host, pages::Page, widgets::Icon},
};

fn clock(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Where this player's sound comes out, by name.
fn output(h: &HostState, p: &Player) -> String {
    let sink = p
        .stream
        .and_then(|id| h.audio.streams.iter().find(|s| s.id == id))
        .and_then(|s| s.sink.clone())
        .unwrap_or_else(|| h.audio.default_sink.clone());
    if h.audio.phone_sink.as_deref() == Some(sink.as_str()) {
        return "This phone".into();
    }
    h.audio
        .sinks
        .iter()
        .find(|s| s.name == sink)
        .map(|s| s.label.clone())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "Speakers".into())
}

/// The whole section, for the computer `key`.
#[component]
pub fn Playing(ui: Ui, pk: String, name: String, h: HostState, may: bool) -> Element {
    let mut pick = use_signal(|| None::<String>);
    if !may {
        return rsx! {
            div { class: "npempty", Icon { name: "music" } "Needs the host permission — see Quick below." }
        };
    }
    if h.media.is_empty() {
        return rsx! {
            div { class: "npempty", Icon { name: "music" } "Nothing is playing on {name}." }
        };
    }
    let p = pick()
        .and_then(|b| h.media.iter().find(|p| p.bus == b).cloned())
        .unwrap_or_else(|| h.media[0].clone());
    let others: Vec<Player> = h.media.iter().filter(|o| o.bus != p.bus).cloned().collect();
    rsx! {
        Card { ui, key: "{p.bus}", host: pk.clone(), h: h.clone(), p: p.clone() }
        if !others.is_empty() {
            div { class: "mediarows",
                for o in others {
                    {
                        let (a, b, k) = (o.clone(), o.clone(), pk.clone());
                        rsx! {
                            div { key: "{o.bus}", class: "mediarow",
                                button { class: "mediarowpick", onclick: move |_| pick.set(Some(a.bus.clone())),
                                    if let Some(src) = o.icon.clone() { img { class: "mediaicon", src: "{src}", alt: "" } }
                                    span { class: "navtext",
                                        b { if o.title.is_empty() { "{o.app}" } else { "{o.title}" } }
                                        small { if o.artist.is_empty() { "{o.app}" } else { "{o.artist} · {o.app}" } }
                                    }
                                }
                                button {
                                    class: "iconbtn",
                                    "aria-label": if o.playing { "Pause" } else { "Play" },
                                    onclick: move |_| host::act_on(ui, &k, HostAction::Media { bus: b.bus.clone(), op: MediaOp::PlayPause }),
                                    Icon { name: if o.playing { "pause" } else { "play" }, fill: true }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The big card for one player. Keyed by the player, so its cover memory is its own.
#[component]
fn Card(ui: Ui, host: String, h: HostState, p: Player) -> Element {
    // The cover under the new one while it fades in.
    let art = p.art.clone().or(p.icon.clone()).unwrap_or_default();
    let mut covers = use_signal(|| (art.clone(), String::new()));
    if covers.peek().0 != art {
        let was = covers.peek().0.clone();
        covers.set((art.clone(), was));
    }
    let (cur, under) = covers();
    let icon_only = p.art.is_none() && p.icon.is_some();

    let (k, bus) = (host.clone(), p.bus.clone());
    let send = move |op: MediaOp| {
        host::act_on(
            ui,
            &k,
            HostAction::Media {
                bus: bus.clone(),
                op,
            },
        )
    };
    let (s1, s2, s3, s4, s5, s6) = (
        send.clone(),
        send.clone(),
        send.clone(),
        send.clone(),
        send.clone(),
        send.clone(),
    );
    let frac = if p.length_ms > 0 {
        (p.position_ms as f64 / p.length_ms as f64).min(1.0)
    } else {
        0.0
    };
    let left_s = p.length_ms.saturating_sub(p.position_ms) as f64 / 1000.0;
    let next_loop = match p.loop_status.as_deref() {
        Some("None") => "Playlist",
        Some("Playlist") => "Track",
        _ => "None",
    };
    let out = output(&h, &p);
    let title = if p.title.is_empty() {
        p.app.clone()
    } else {
        p.title.clone()
    };

    rsx! {
        div { class: "np",
            div { class: "nptop",
                div { class: if icon_only { "npart icon" } else { "npart" },
                    if !under.is_empty() && under != cur {
                        i { key: "u{under}", class: "under", style: "background-image:url(\"{under}\")" }
                    }
                    if !cur.is_empty() {
                        i { key: "c{cur}", class: "cur", style: "background-image:url(\"{cur}\")" }
                    } else {
                        span { class: "npnoart", Icon { name: "music" } }
                    }
                }
                div { class: "npt",
                    span { class: "npapp",
                        if let Some(src) = p.icon.clone() { img { class: "mediaicon", src: "{src}", alt: "" } }
                        "{p.app}"
                    }
                    b { key: "{title}", class: "swapin", "{title}" }
                    if !p.artist.is_empty() { span { key: "{p.artist}", class: "ar swapin", "{p.artist}" } }
                }
            }
            if p.length_ms > 0 {
                div {
                    div { class: "scrub",
                        span { class: "track",
                            span {
                                key: "{p.position_ms}-{p.playing}",
                                class: if p.playing { "prog run" } else { "prog" },
                                style: "--from:{frac:.4};--left:{left_s:.1}s",
                            }
                        }
                        if p.can_seek {
                            input {
                                r#type: "range", min: "0", max: "{p.length_ms}", step: "1000",
                                "aria-label": "Position", value: "{p.position_ms}",
                                onchange: move |e| if let Ok(ms) = e.value().parse::<u64>() { s1(MediaOp::SeekTo { ms }) },
                            }
                        }
                    }
                    div { class: "times", span { "{clock(p.position_ms)}" } span { "{clock(p.length_ms)}" } }
                }
            }
            div { class: "npctl",
                button {
                    class: "npout",
                    "aria-label": "Plays on {out} — change",
                    onclick: move |_| route::go(ui, Page::SoundDevices),
                    Icon { name: if out == "This phone" { "phone" } else { "sound" } }
                    span { "{out}" }
                }
                span { class: "npbtns",
                    if let Some(sh) = p.shuffle {
                        button { class: if sh { "iconbtn small on" } else { "iconbtn small" }, "aria-label": "Shuffle", "aria-pressed": "{sh}",
                            onclick: move |_| s5(MediaOp::Shuffle { on: !sh }), Icon { name: "shuffle" } }
                    }
                    button { class: "iconbtn", "aria-label": "Previous", disabled: !p.can_prev,
                        onclick: move |_| s2(MediaOp::Previous), Icon { name: "prev-f", fill: true } }
                    button { class: "npplay", "aria-label": if p.playing { "Pause" } else { "Play" },
                        onclick: move |_| s3(MediaOp::PlayPause),
                        span { key: "{p.playing}", class: "glyph", Icon { name: if p.playing { "pause" } else { "play" }, fill: true } }
                    }
                    button { class: "iconbtn", "aria-label": "Next", disabled: !p.can_next,
                        onclick: move |_| s4(MediaOp::Next), Icon { name: "next-f", fill: true } }
                    if let Some(l) = p.loop_status.clone() {
                        button {
                            class: if l == "None" { "iconbtn small" } else { "iconbtn small on" },
                            "aria-label": "Repeat: {l}",
                            onclick: move |_| s6(MediaOp::Loop { mode: next_loop.into() }),
                            Icon { name: if l == "Track" { "repeat1" } else { "repeat" } }
                        }
                    }
                }
            }
        }
    }
}
