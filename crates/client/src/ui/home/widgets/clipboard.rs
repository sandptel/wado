//! The Clipboard widget: the newest three on its computer's clipboard, pinned first, each one tap
//! onto this phone; and this phone's clipboard sent the other way. Its title opens the full rail.

use dioxus::prelude::*;
use wado_protocol::ClipEntry;

use crate::{bridge, state::Ui, ui::widgets::Icon};

use super::shell::{asking, Ctx};

/// The full clipboard rail, for the computer `pk` — which becomes the selected one.
pub fn open_rail(ui: Ui, pk: &str) {
    if let Some(i) = crate::route::index_of(ui, pk) {
        if (ui.set.profile)() != i {
            crate::profile::select(ui, i);
        }
    }
    let mut open = ui.live.clip_rail;
    open.set(true);
}

#[component]
pub fn Clipboard(cx: Ctx) -> Element {
    let ui = cx.ui;
    let entries: Vec<ClipEntry> = if cx.r.selected {
        let h = (ui.live.clip_hist)();
        if !h.loaded {
            return asking(&cx.name);
        }
        h.entries
    } else {
        match ui.live.side.read().get(&cx.w.host) {
            Some(s) if s.state == "up" => s.clips.clone(),
            _ => return asking(&cx.name),
        }
    };
    let mut top: Vec<ClipEntry> = entries.iter().filter(|e| e.pinned).cloned().collect();
    top.extend(entries.iter().filter(|e| !e.pinned).cloned());
    top.truncate(3);
    let (pk, sel) = (cx.w.host.clone(), cx.r.selected);
    rsx! {
        div { class: "clipsw",
            if top.is_empty() { div { class: "wnote quiet", Icon { name: "clip" } "Nothing copied on {cx.name} yet." } }
            for e in top {
                Entry { key: "{e.id}", e, pk: pk.clone(), selected: sel }
            }
        }
        button {
            class: "btn sendclip",
            onclick: move |_| bridge::call(if sel {
                "window.__wado.clipPaste();".to_string()
            } else {
                format!("window.__wado.sidePaste({});", bridge::js(&pk))
            }),
            Icon { name: "paste" } "Send this phone's clipboard"
        }
    }
}

#[component]
fn Entry(e: ClipEntry, pk: String, selected: bool) -> Element {
    let mut copied = use_signal(|| false);
    let kind = serde_json::to_value(e.kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    let id = e.id.clone();
    rsx! {
        div { class: if copied() { "clipw copied" } else { "clipw" },
            if e.pinned { span { class: "pin", Icon { name: "pin" } } }
            span { class: "ct", "{e.preview}" }
            button {
                class: "iconbtn",
                "aria-label": if copied() { "Copied" } else { "Copy to this phone" },
                onclick: move |_| {
                    bridge::call(if selected {
                        format!("window.__wado.clipCopy({}, {});", bridge::js(&id), bridge::js(&kind))
                    } else {
                        format!("window.__wado.sideClipCopy({}, {}, {});", bridge::js(&pk), bridge::js(&id), bridge::js(&kind))
                    });
                    copied.set(true);
                    spawn(async move {
                        bridge::sleep(1400).await;
                        copied.set(false);
                    });
                },
                span { key: "{copied()}", class: "glyph", Icon { name: if copied() { "check" } else { "copy" } } }
            }
        }
    }
}
