//! The clipboard's voice, over everything: one polite live region at the bottom of the screen
//! that says what sharing did ("Sent to Laptop", "Copied to this phone from Laptop"), and holds
//! the one-tap chip a browser asks for — a computer's copy ready to put on this phone, or this
//! phone's ready to send. Mounted once, at the app's root, so it speaks over home, a computer's
//! page and a running session alike.
//!
//! It also tells `js/clip_share.js` which computers there are links to and what each may — the
//! selected one, and every light link that is up.

use dioxus::prelude::*;

use crate::{bridge, reach, state::Ui, ui::widgets::Icon};

/// How long a confirmation stays, in ms. Long enough to read twice; it never holds anything the
/// person must act on (that is the chip, which waits).
const SAID_MS: u32 = 2800;

#[derive(serde::Serialize)]
struct Link {
    key: String,
    name: String,
    selected: bool,
    may: bool,
    paired: bool,
}

pub fn render(ui: Ui) -> Element {
    links(ui);
    let chip = (ui.live.clip_chip)();
    let said = (ui.live.clip_said)();
    let mut shown = use_signal(|| 0u32);
    // A confirmation shows for a moment, then goes; the next one restarts it.
    use_effect(move || {
        let n = (ui.live.clip_said)().n;
        if n == 0 {
            return;
        }
        shown.set(n);
        spawn(async move {
            bridge::sleep(SAID_MS).await;
            if *shown.peek() == n {
                shown.set(0);
            }
        });
    });
    let saying = shown() != 0 && shown() == said.n && !said.text.is_empty();

    rsx! {
        div {
            id: "clipdock",
            class: if (ui.live.session_on)() { "insession" } else { "" },
            role: "status",
            "aria-live": "polite",
            "aria-atomic": "true",
            if let Some(c) = chip {
                {chip_view(c)}
            } else if saying {
                div { key: "said{said.n}", class: "clipsaid {said.tone}",
                    span { class: "csico", "aria-hidden": "true",
                        Icon { name: if said.tone == "bad" { "alert" } else if said.tone == "dim" { "info" } else { "check" } }
                    }
                    span { "{said.text}" }
                }
            }
        }
    }
}

fn chip_view(c: crate::state::ClipChip) -> Element {
    let copy = c.kind == "copy";
    let what = if c.image {
        "a picture".to_string()
    } else if c.preview.is_empty() {
        "text".to_string()
    } else {
        format!("“{}”", c.preview)
    };
    rsx! {
        div { key: "{c.id}", class: if copy { "clipchip2 copy" } else { "clipchip2 send" },
            // Where it goes: computer → phone, or phone → computer, with a dot travelling.
            span { class: "cflow", "aria-hidden": "true",
                Icon { name: if copy { "laptop" } else { "phone" } }
                i { class: "cdot" }
                Icon { name: if copy { "phone" } else { "laptop" } }
            }
            span { class: "ctext",
                b { if copy { "Copied on {c.name}" } else { "Send this phone's clipboard?" } }
                small { if copy { "{what}" } else { "To {c.name}" } }
            }
            button {
                class: "cgo",
                "data-clip": if copy { "chip-copy" } else { "chip-send" },
                "data-key": "{c.key}",
                "aria-label": if copy { "Copy to this phone" } else { "Send to {c.name}" },
                if copy { "Copy" } else { "Send" }
            }
            button {
                class: "cno",
                "data-clip": "chip-close",
                "aria-label": "Not now",
                Icon { name: "x" }
            }
        }
    }
}

/// Keep `js/clip_share.js` told of the computers there are links to now.
fn links(ui: Ui) {
    // What was said last: the effect re-runs whenever a link's state moves (every ping), and
    // only a change is worth a call.
    let last = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(String::new())));
    use_effect(move || {
        let profiles = ui.set.profiles.read().clone();
        let cur = (ui.set.profile)();
        let side = ui.live.side.read().clone();
        let list: Vec<Link> = profiles
            .iter()
            .enumerate()
            .filter(|(_, p)| p.conn_mode == "relay")
            .filter_map(|(i, p)| {
                let key = p.key();
                let selected = i == cur;
                if !selected && side.get(&key).is_none_or(|s| s.state != "up") {
                    return None;
                }
                let r = reach::of(ui, &key);
                let g = r.grants();
                Some(Link {
                    name: p.display(),
                    selected,
                    may: r.may("clipboard"),
                    paired: g.is_some_and(|g| g.paired && g.clipboard),
                    key,
                })
            })
            .collect();
        let json = bridge::js(&list);
        if *last.borrow() != json {
            bridge::call(format!("window.__wado.clipLinks({json});"));
            *last.borrow_mut() = json;
        }
    });
}
