//! The Quick actions face: what is pressed often in a session — the next and previous workspace,
//! and playback on the computer — plus what is playing (`.npmini`). Quick *tiles* stay in the
//! control centre: those are this device's toggles, touched rarely.
//!
//! Playback is the computer's media player (`HostAction::Media`), which needs the `host` grant;
//! without it the face says so rather than offering buttons the daemon would refuse.

use dioxus::prelude::*;
use serde_json::json;
use wado_protocol::host::{HostAction, MediaOp};

use crate::{
    state::Ui,
    ui::{host, widgets::Icon, workspaces},
};

/// The player an action goes to: the one playing, else the first.
fn player(ui: Ui) -> Option<wado_protocol::host::Player> {
    let st = (ui.live.hoststate)();
    let s = st.as_ref()?;
    s.media
        .iter()
        .find(|p| p.playing)
        .or_else(|| s.media.first())
        .cloned()
}

fn media(ui: Ui, op: MediaOp) {
    if let Some(p) = player(ui) {
        host::act(HostAction::Media { bus: p.bus, op });
    }
}

pub fn render(ui: Ui) -> Element {
    let (_, active) = (ui.live.windows)();
    let ws = active.max(1);
    let now = player(ui);
    let playing = now.as_ref().is_some_and(|p| p.playing);
    // No grants from an older daemon: offer it and let the daemon say no.
    let granted = (ui.live.host)().grants.map(|g| g.host).unwrap_or(true);
    rsx! {
        div { class: "dquick",
            button {
                class: "tpact",
                disabled: ws <= 1,
                onclick: move |_| workspaces::act(json!({ "workspace": { "n": ws.saturating_sub(1).max(1) } })),
                Icon { name: "left" }
                span { class: "tplabel", "Previous workspace" }
            }
            button {
                class: "tpact",
                onclick: move |_| workspaces::act(json!({ "workspace": { "n": ws + 1 } })),
                Icon { name: "right" }
                span { class: "tplabel", "Next workspace" }
            }
            if granted {
                button {
                    class: "tpact",
                    disabled: now.is_none(),
                    onclick: move |_| media(ui, MediaOp::PlayPause),
                    Icon { name: if playing { "pause" } else { "play" }, fill: true }
                    span { class: "tplabel", "Play / pause" }
                }
            } else {
                p { class: "dnote", "Playback needs the host grant. On the computer: wado allow <this device> host" }
            }
        }
        if let Some(p) = now.filter(|_| granted) {
            div { class: "npmini", "aria-label": "Now playing on the computer",
                if let Some(art) = p.art.clone() {
                    img { class: "cv", src: "{art}", alt: "" }
                } else {
                    span { class: "cv", Icon { name: "play", fill: true } }
                }
                span { class: "nt",
                    b { "{p.title}" }
                    small { "{p.artist}" }
                }
                div { class: "ctl",
                    button { "aria-label": "Previous track", onclick: move |_| media(ui, MediaOp::Previous),
                        Icon { name: "prev-f", fill: true }
                    }
                    button { "aria-label": "Next track", onclick: move |_| media(ui, MediaOp::Next),
                        Icon { name: "next-f", fill: true }
                    }
                }
            }
        }
    }
}
