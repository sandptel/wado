//! What to do when this phone may not do something here: the exact command that allows it, one
//! tap to copy, and the `wado tui` way for someone who would rather tick a box. Never a dead
//! control — a missing grant is said, with its fix.

use dioxus::prelude::*;

use crate::{bridge, ui::widgets::Icon};

#[component]
pub fn Grant(
    /// The computer's name.
    computer: String,
    /// This phone as `wado allow` wants it typed.
    device: String,
    /// The grant, as `wado allow` names it.
    token: String,
    /// What it unlocks, in words: "its sound, media, brightness or radios".
    what: String,
    /// What this phone may already do there, in words; empty when nothing.
    has: String,
) -> Element {
    let mut copied = use_signal(|| false);
    let cmd = format!("wado allow {device} {token}");
    let c2 = cmd.clone();
    rsx! {
        div { class: "grant",
            b { "Quick controls need permission" }
            p {
                if has.is_empty() { "This phone may not control {what} on {computer} yet. " }
                else { "This phone may use {computer}'s {has}, but not {what}. " }
                "To allow it, run this on {computer}:"
            }
            div { class: "cmd",
                code { "{cmd}" }
                button {
                    class: if copied() { "iconbtn copied" } else { "iconbtn" },
                    "aria-label": if copied() { "Copied" } else { "Copy the command" },
                    onclick: move |_| {
                        bridge::call(format!("window.__wado.clipboardToPhone({});", bridge::js(&c2)));
                        copied.set(true);
                        spawn(async move {
                            bridge::sleep(1400).await;
                            copied.set(false);
                        });
                    },
                    span { key: "{copied()}", class: "glyph", Icon { name: if copied() { "check" } else { "copy" } } }
                }
            }
            p { class: "why", "Or open " span { class: "mono", "wado tui" } " there and tick “{token}” for this phone." }
        }
    }
}

/// The grants a device has, as words for "may use X's …".
pub fn has_words(g: &wado_protocol::DeviceGrants) -> String {
    let mut out = Vec::new();
    if g.files != "none" {
        out.push("files");
    }
    if g.shells {
        out.push("shells");
    }
    if g.input {
        out.push("trackpad");
    }
    if g.clipboard {
        out.push("clipboard");
    }
    match out.len() {
        0 => String::new(),
        1 => out[0].to_string(),
        n => format!("{} and {}", out[..n - 1].join(", "), out[n - 1]),
    }
}
