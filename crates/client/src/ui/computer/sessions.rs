//! The Computer page's Sessions section (Decision Log `2026-10-30`): every session this computer
//! runs, on whichever daemon of its pool, with who is in each — and the way in to each: Rejoin,
//! Join (everyone controls it at once), Take over (it becomes yours, at your size; the others are
//! told), End (anyone in it may, for everyone). Then Start new, and the link test that says
//! whether any of it would work from here.
//!
//! An older relay or daemon cannot join: the row says why, and Take over and End go the way they
//! always went.

use dioxus::prelude::*;

use crate::{
    bridge,
    multi::offer,
    reach::Reach,
    state::Ui,
    ui::{
        computer::{linkmeter::LinkMeter, session_row::SessionRow},
        widgets::Icon,
    },
};

#[component]
pub fn Sessions(ui: Ui, r: Reach) -> Element {
    let live = ui.live;
    // Asked while this page is up and no session is on screen — the list is looked at here.
    use_effect(move || {
        let watch = !(live.session_on)();
        bridge::call(format!("window.__wado.sessionsWatch({watch});"));
    });
    let (here, list) = (live.sessions)();
    let m = (live.multi)();
    let rooms = m.rooms();
    let granted = r.grants().map(|g| (g.join, g.takeover));
    let in_session = !m.list.is_empty();
    let intent = if list.is_empty() { "start" } else { "join" };
    let starting = m.busy.1 == "start";
    let host = m.verdict.as_ref().map(|v| &v.host);
    let free = host
        .filter(|h| h.max_sessions > 0)
        .map(|h| h.max_sessions.saturating_sub(h.sessions));
    let full = free == Some(0);

    rsx! {
        section { class: "sx sessions2 o2", style: "--n:2", "aria-label": "Sessions",
            div { class: "sect",
                span { "Sessions" }
                if !list.is_empty() {
                    span { class: "sectcount mono", "{list.len()}" }
                }
            }
            if !r.online {
                div { class: "npempty", Icon { name: "monitor" } "Sessions show once it answers." }
            } else {
                if list.is_empty() {
                    div { class: "sempty",
                        span { class: "semptyart", "aria-hidden": "true", i {} i {} i {} }
                        p { "Nothing running. A session starts at this screen's size; others can join it once it runs." }
                    }
                }
                div { class: "srows",
                    for (i, s) in list.iter().cloned().enumerate() {
                        {
                            let you = if s.instance == here && in_session { m.you } else { 0 };
                            let in_it = you != 0 && s.participants.iter().any(|p| p.id == you);
                            let o = offer(&s, &here, in_it, rooms, granted);
                            let busy = if m.busy.0 == s.instance { m.busy.1.clone() } else { String::new() };
                            rsx! {
                                SessionRow { key: "{s.instance}", ui, s, offer: o, i, you, busy }
                            }
                        }
                    }
                }
                LinkMeter { ui, intent: intent.to_string() }
                button {
                    class: "btn snew",
                    disabled: starting || full,
                    onclick: move |_| bridge::call("window.__wado.multiStart();".to_string()),
                    Icon { name: "plus" }
                    if starting {
                        "Finding a place for it…"
                    } else if full {
                        "Every session it runs is in use"
                    } else {
                        "Start a new session"
                    }
                    if let (Some(f), Some(h)) = (free, host) {
                        if !full && !starting {
                            small { class: "mono", "{f} of {h.max_sessions} free" }
                        }
                    }
                }
                if !rooms && !list.is_empty() {
                    p { class: "swhy", Icon { name: "info" }
                        "This relay is older than joining: one device per session, and Take over moves it to you."
                    }
                }
            }
        }
    }
}
