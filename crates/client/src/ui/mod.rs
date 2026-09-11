//! The settings panel: which groups exist and in what order.
//!
//! Groups are ordered by **when a setting takes effect**, not by subsystem — connection
//! first, then what is read at Start, then what applies instantly, then appearance and debug.
//! That ordering is also the reason the old `"(applies at Start)"` suffixes are gone: the
//! group says it once, and the Session group disables itself while a session runs so the rule
//! is enforced rather than merely written down.

pub mod appearance;
pub mod bar;
pub mod connection;
pub mod debug;
pub mod launcher;
pub mod live;
pub mod session;
pub mod stage;
pub mod status;
pub mod term;

use dioxus::prelude::*;

use crate::{actions, state::Ui};

/// A collapsible settings group. `note` is the one-line explanation of *when* it applies.
#[component]
pub fn Group(title: String, note: String, open: bool, children: Element) -> Element {
    rsx! {
        details { class: "group", open,
            summary { "{title}" }
            p { class: "hint note", "{note}" }
            div { class: "group-body", {children} }
        }
    }
}

pub fn panel(ui: Ui) -> Element {
    let on = (ui.live.session_on)();
    rsx! {
        header { class: "brand",
            h1 { "wado" }
            p { class: "hint", "Configure and start a streaming session." }
        }

        {connection::render(ui)}

        {status::render(ui)}

        Group {
            title: "Session",
            note: if on { "Locked while a session is running." } else { "Read once, at Start." },
            open: true,
            {session::render(ui)}
        }

        Group { title: "Live", note: "Applies immediately.", open: true,
            {live::render(ui)}
        }

        Group { title: "Appearance", note: "Applies immediately.", open: false,
            {appearance::render(ui)}
        }

        Group { title: "Debug", note: "View-only. Off also stops the work behind it.", open: false,
            {debug::render(ui)}
        }

        div { class: "btns",
            button { id: "start", disabled: on, onclick: move |_| actions::start(ui), "Start" }
            button { id: "stop", disabled: !on, onclick: move |_| actions::stop(ui), "Stop" }
        }
        div { id: "status", "{(ui.live.status)()}" }
    }
}
