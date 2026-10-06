//! One clipboard entry: what it is, pin and delete, and its text or picture. Tapping it copies it
//! to this device.

use dioxus::prelude::*;
use wado_protocol::{ClipEntry, ClipKind};

use crate::{bridge, ui::widgets::Icon};

/// How long the collapse animation of a deleted tile runs (`clips.css`), before it is deleted.
const LEAVE_MS: u32 = 240;

#[component]
pub fn Tile(entry: ClipEntry, img: Option<String>, i: usize) -> Element {
    let mut leaving = use_signal(|| false);
    let id = entry.id.clone();
    let image = entry.kind == ClipKind::Image;
    {
        let id = id.clone();
        use_effect(move || {
            if image {
                bridge::call(format!("window.__wado.clipThumb({});", bridge::js(&id)));
            }
        });
    }
    let (icon, label, kind) = match entry.kind {
        ClipKind::Text => ("text", "Text", "text"),
        ClipKind::Link => ("link", "Link", "link"),
        ClipKind::Image => ("image", "Image", "image"),
    };
    let pinned = entry.pinned;
    let class = format!(
        "cliptile {kind}{}{}",
        if pinned { " pinned" } else { "" },
        if leaving() { " leaving" } else { "" }
    );
    let (copy_id, pin_id, del_id) = (id.clone(), id.clone(), id.clone());

    rsx! {
        div {
            class: "{class}",
            "data-id": "{id}",
            style: "--i:{i.min(14)}",
            role: "button",
            title: "Copy to this device",
            onclick: move |_| bridge::call(format!(
                "window.__wado.clipCopy({}, {});",
                bridge::js(&copy_id),
                bridge::js(&kind)
            )),
            div { class: "cliphead",
                Icon { name: icon }
                span { "{label}" }
                button {
                    class: if pinned { "clipbtn on" } else { "clipbtn" },
                    "aria-label": if pinned { "Unpin" } else { "Pin" },
                    onclick: move |e| {
                        e.stop_propagation();
                        bridge::call(format!(
                            "window.__wado.clipPin({}, {});",
                            bridge::js(&pin_id),
                            !pinned
                        ));
                    },
                    Icon { name: "pin" }
                }
                button {
                    class: "clipbtn",
                    "aria-label": "Delete",
                    onclick: move |e| {
                        e.stop_propagation();
                        leaving.set(true);
                        bridge::call(format!(
                            "setTimeout(() => window.__wado.clipDelete({}), {LEAVE_MS});",
                            bridge::js(&del_id)
                        ));
                    },
                    Icon { name: "trash" }
                }
            }
            if image {
                div { class: "clippic",
                    if let Some(src) = img { img { src: "{src}", alt: "{entry.preview}" } }
                }
                small { class: "clipmeta", "{entry.preview}" }
            } else {
                p { class: "clipbody", "{entry.preview}" }
            }
        }
    }
}
