//! One application tile: icon, name, and the two ways to press it.
//!
//! Its own file because the press rules are the fiddly part of the drawer and they are all
//! here: tap launches and closes the sheet, long-press (a `contextmenu` on touch, a
//! right-click on a desktop) puts the command in the box instead of running it.
//!
//! A running application is marked with a dot *and* said in the tile's title, because a dot
//! is a colour and colour alone is not a label — see [`wado_protocol::AppEntry::running`] for
//! what "running" is taken to mean.

use dioxus::prelude::*;
use wado_protocol::AppEntry;

use crate::{actions, state::Ui};

/// Render one tile.
pub fn render(ui: Ui, app: &AppEntry) -> Element {
    let mut live = ui.live;
    let mut s = ui.set;
    let exec = app.exec.clone();
    let for_press = exec.clone();
    // A letter is a better placeholder than a generic glyph: it differs per app, so the tiles
    // stay distinguishable at a glance even on a machine with no icon theme installed.
    let letter = app
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();

    let running = app.running;
    let title = if running {
        format!("{} — running", app.exec)
    } else {
        app.exec.clone()
    };

    rsx! {
        button {
            key: "{app.exec}",
            class: if running { "apptile running" } else { "apptile" },
            title: "{title}",
            onclick: move |_| {
                actions::launch_command(ui, &exec);
                live.drawer_open.set(false);
            },
            // Long press on touch, right-click on a desktop. The default here is the browser's
            // own context menu, which over a video is never what was wanted.
            oncontextmenu: move |e| {
                e.prevent_default();
                s.command.set(for_press.clone());
            },
            // The dot is positioned against this wrapper, not the tile, so it sits on the
            // icon's corner whatever the icon's own aspect ratio turns out to be.
            span { class: "appiconwrap",
                if let Some(icon) = app.icon.as_ref() {
                    img { class: "appicon", src: "{icon}", alt: "" }
                } else {
                    span { class: "appicon appletter", "{letter}" }
                }
                if running {
                    span { class: "appdot" }
                }
            }
            span { class: "appname", "{app.name}" }
        }
    }
}
