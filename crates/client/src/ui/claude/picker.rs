//! "New Claude session": the sheet that walks the computer's folders to the project, then starts
//! Claude there. Folders Claude was used in lately are one tap away at the top.

use dioxus::prelude::*;

use crate::{
    bridge,
    claude::{DIRS, PICKER},
    state::Ui,
    ui::widgets::Icon,
};

fn base(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("/")
        .to_string()
}

fn go(path: &str) {
    bridge::call(format!("window.__wado.claudeDirs({});", bridge::js(&path)));
}

pub fn render(ui: Ui) -> Element {
    let open = PICKER();
    let d = DIRS();
    let name = crate::profile::current_name(ui);
    let close = move |_| *PICKER.write() = false;
    let here = d.as_ref().map(|d| d.path.clone()).unwrap_or_default();
    let h2 = here.clone();
    rsx! {
        div { class: if open { "pkscrim up" } else { "pkscrim" }, onclick: close }
        section {
            class: if open { "pksheet clpick up" } else { "pksheet clpick" },
            role: "dialog",
            "aria-modal": "true",
            "aria-labelledby": "cpicktitle",
            "aria-hidden": "{!open}",
            inert: !open,
            div { class: "grab", onclick: close }
            div { class: "pkhead",
                h2 { id: "cpicktitle", "New Claude session" }
            }
            p { class: "pklead", "Pick the project folder on {name}. Claude starts there, and its shell opens here." }
            match d {
                None => rsx! { div { class: "npempty", Icon { name: "folder" } "Reading {name}'s folders…" } },
                Some(d) if !d.error.is_empty() => rsx! { p { class: "pknote", "{d.error}" } },
                Some(d) => rsx! {
                    if !d.recent.is_empty() {
                        div { class: "clrecent", "aria-label": "Recent projects",
                            for r in d.recent.clone() {
                                button { key: "{r}", class: "chip",
                                    onclick: { let r = r.clone(); move |_| go(&r) },
                                    Icon { name: "folder" } "{base(&r)}"
                                }
                            }
                        }
                    }
                    div { class: "clpath",
                        button { class: "iconbtn", disabled: d.parent.is_empty(), "aria-label": "Up one folder",
                            onclick: { let p = d.parent.clone(); move |_| go(&p) },
                            Icon { name: "up" }
                        }
                        span { class: "mono", "{d.path}" }
                    }
                    div { class: "cldirs", role: "list",
                        if d.dirs.is_empty() {
                            p { class: "why", "No folders in here." }
                        }
                        for f in d.dirs.clone() {
                            button { key: "{f}", class: "cldir", role: "listitem",
                                onclick: {
                                    let p = format!("{}/{}", d.path.trim_end_matches('/'), f);
                                    move |_| go(&p)
                                },
                                Icon { name: "folder" } span { "{f}" } Icon { name: "right" }
                            }
                        }
                    }
                },
            }
            div { class: "pkacts",
                button {
                    class: "btn primary",
                    disabled: here.is_empty(),
                    onclick: move |_| bridge::call(format!("window.__wado.claudeNew({});", bridge::js(&h2))),
                    Icon { name: "play", fill: true } "Start Claude in {base(&here)}"
                }
                button { class: "btn", onclick: close, "Cancel" }
            }
        }
    }
}
