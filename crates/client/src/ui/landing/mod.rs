//! Before a session: who to connect to, how far the connection got, and Start.
//!
//! A page of its own rather than the control centre opened over nothing, because before a
//! session the only decision is *where*, and it deserves the whole screen. It dissolves into
//! the stream once the session is up.

mod profiles;
mod progress;
mod sessions;

use dioxus::prelude::*;

use crate::{
    actions,
    state::Ui,
    ui::{pages::Page, widgets::Icon},
};

pub fn render(ui: Ui) -> Element {
    let on = (ui.live.session_on)();
    let mut live = ui.live;
    let s = ui.set;
    let c = crate::cfg::build(ui);
    let status = (live.status)();
    let has_sessions = !(live.sessions)().1.is_empty();

    rsx! {
        section { id: "landing", class: if on { "gone" } else { "" }, "aria-hidden": "{on}",
            div { class: "wordmark", "wado" span { "." } }
            p { class: "tag", "Your desktop, at this screen's own resolution." }
            {progress::render(ui)}
            {crate::ui::scoped(ui, sessions::render)}
            {crate::ui::scoped(ui, profiles::render)}
            {crate::ui::scoped(ui, crate::ui::media::render)}
            button {
                class: "shapecard",
                onclick: move |_| { live.cc_page.set(Page::Display); live.cc_open.set(true); },
                span { class: "disc", style: "--hue:var(--base0D)", Icon { name: "monitor" } }
                span { class: "navtext",
                    b { "{c.width}×{c.height} · {c.fps} fps · {c.scale}×" }
                    small { "{(s.quality)()} · tap to change" }
                }
                Icon { name: "right" }
            }
            div { class: "landingfoot",
                button {
                    class: "iconbtn",
                    "aria-label": "Settings",
                    onclick: move |_| { live.cc_page.set(Page::Root); live.cc_open.set(true); },
                    Icon { name: "sliders" }
                }
                // Shell-only: a terminal over the relay with no compositor or encoder running.
                if (s.conn_mode)() == "relay" {
                    button {
                        class: "iconbtn",
                        "aria-label": "Open a shell — no video",
                        title: "Open a shell — no video",
                        onclick: move |_| crate::ui::console::open_shells(ui),
                        Icon { name: "term" }
                    }
                }
                if has_sessions {
                    // A session is already running: joining it is the likely wish, a new one the
                    // other. Both on the one button row, Join the bigger.
                    button { class: "gonew", disabled: on, title: "Start a new session", "aria-label": "New session",
                        onclick: move |_| sessions::new(ui),
                        Icon { name: "plus" } "New"
                    }
                    button { class: "go", disabled: on, onclick: move |_| sessions::join(ui),
                        Icon { name: "play" } "Join session"
                    }
                } else {
                    button { class: "go", disabled: on, onclick: move |_| actions::start(ui),
                        Icon { name: "power" } "Start session"
                    }
                }
            }
            if status != "idle" { p { class: "landingstatus", "{status}" } }
        }
    }
}
