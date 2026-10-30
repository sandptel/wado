//! One clipboard entry: what it is and where it came from, its text or picture, and its actions —
//! Copy to this phone, Send back to the computer's clipboard, and (on the full surface) Pin and
//! Delete. Copy and Send are taken by `js/clip_share.js` inside the tap (`data-clip`), where a
//! browser lets the clipboard be used; their tick is drawn from `data-done`.

use dioxus::prelude::*;
use wado_protocol::{ClipEntry, ClipKind};

use crate::{bridge, ui::widgets::Icon};

use super::words;

/// How long a deleted tile takes to fold away (`clips.css`), before it is deleted.
const LEAVE_MS: u32 = 260;

#[component]
pub fn Tile(
    entry: ClipEntry,
    /// The computer, by key and by name.
    pk: String,
    computer: String,
    /// Reached through the selected computer's link (else its light link).
    selected: bool,
    img: Option<String>,
    /// Its place in the list, for the entrance's stagger.
    i: usize,
    /// The widget's short form: no pin, no delete.
    compact: bool,
) -> Element {
    let mut leaving = use_signal(|| false);
    let id = entry.id.clone();
    let image = entry.kind == ClipKind::Image;
    {
        let (id, pk) = (id.clone(), pk.clone());
        use_effect(move || {
            if image {
                bridge::call(if selected {
                    format!("window.__wado.clipThumb({});", bridge::js(&id))
                } else {
                    format!(
                        "window.__wado.sideClipThumb({}, {});",
                        bridge::js(&pk),
                        bridge::js(&id)
                    )
                });
            }
        });
    }
    let (icon, kind) = match entry.kind {
        ClipKind::Text => ("text", "text"),
        ClipKind::Link => ("link", "link"),
        ClipKind::Image => ("image", "image"),
    };
    let pinned = entry.pinned;
    // Copied in the last few seconds: it arrives with a mark.
    let fresh = entry.at_ms > 0 && crate::profile::now_s().saturating_sub(entry.at_ms / 1000) < 6;
    let class = format!(
        "cliptile {kind}{}{}{}",
        if pinned { " pinned" } else { "" },
        if leaving() { " leaving" } else { "" },
        if fresh { " fresh" } else { "" },
    );
    let label = words::label(&entry, &computer);
    let meta = words::meta(&entry, &computer);
    let (pin_id, del_id, pk_pin, pk_del) = (id.clone(), id.clone(), pk.clone(), pk.clone());
    let short: String = entry.preview.chars().take(40).collect();

    rsx! {
        li {
            class: "{class}",
            "data-id": "{id}",
            style: "--i:{i.min(12)}",
            // The fold collapses a deleted entry to nothing; the card is what is seen.
            div { class: "clipfold", div { class: "clipcard",
                // Said whole, once; the drawn parts below are for the eye.
                span { class: "sr", "{label}" }
                div { class: "clipmain", "aria-hidden": "true",
                    span { class: "clipico", "aria-hidden": "true", Icon { name: icon } }
                    div { class: "cliptxt",
                        if image {
                            div { class: "clippic",
                                if let Some(src) = img { img { src: "{src}", alt: "{entry.preview}" } }
                            }
                            small { class: "clipdim mono", "{entry.preview}" }
                        } else {
                            p { class: "clipbody", "{entry.preview}" }
                        }
                    }
                }
                // Where and when, then what can be done with it (side by side on a wide card).
                div { class: "clipfoot",
                if pinned || !meta.is_empty() {
                    small { class: "clipmeta", "aria-hidden": "true",
                        if pinned { span { class: "clippin", Icon { name: "pin" } "Pinned" } }
                        "{meta}"
                    }
                }
                div { class: "clipacts",
                    button {
                        class: "clipact go",
                        "data-clip": "copy",
                        "data-key": "{pk}",
                        "data-id": "{id}",
                        "data-kind": "{kind}",
                        "aria-label": "Copy to this phone: {short}",
                        title: "Copy to this phone",
                        span { class: "g g1", Icon { name: "copy" } }
                        span { class: "g g2", Icon { name: "check" } }
                    }
                    button {
                        class: "clipact",
                        "data-clip": "send",
                        "data-key": "{pk}",
                        "data-id": "{id}",
                        "aria-label": "Put on {computer}'s clipboard: {short}",
                        title: "Put on {computer}'s clipboard",
                        span { class: "g g1", Icon { name: "laptop" } }
                        span { class: "g g2", Icon { name: "check" } }
                    }
                    if !compact {
                        button {
                            class: if pinned { "clipact pin on" } else { "clipact pin" },
                            "aria-pressed": "{pinned}",
                            "aria-label": if pinned { "Unpin: {short}" } else { "Pin: {short}" },
                            title: if pinned { "Unpin" } else { "Pin" },
                            onclick: move |_| clip_do(&pk_pin, selected, serde_json::json!({
                                "type": "clip_pin", "id": pin_id, "on": !pinned
                            })),
                            Icon { name: "pin" }
                        }
                        button {
                            class: "clipact del",
                            "aria-label": "Delete from the history: {short}",
                            title: "Delete",
                            onclick: move |_| {
                                leaving.set(true);
                                let msg = serde_json::json!({ "type": "clip_delete", "id": del_id });
                                let send = send_js(&pk_del, selected, &msg);
                                bridge::call(format!("setTimeout(() => {{ {send} }}, {LEAVE_MS});"));
                            },
                            Icon { name: "trash" }
                        }
                    }
                }
                }
            } }
        }
    }
}

/// The JS that sends `msg` to computer `pk` on whichever link carries it.
pub fn send_js(pk: &str, selected: bool, msg: &serde_json::Value) -> String {
    if selected {
        format!("window.__wado.relaySendMsg({});", msg)
    } else {
        format!("window.__wado.sideDo({}, {});", bridge::js(&pk), msg)
    }
}

pub fn clip_do(pk: &str, selected: bool, msg: serde_json::Value) {
    bridge::call(send_js(pk, selected, &msg));
}
