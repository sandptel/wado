//! The host picker: a sheet that sets which computer a widget follows. The chosen row's check
//! pops; the sheet closes a beat later and the widget pops into its new self.

use dioxus::prelude::*;

use crate::{bridge, profile, reach, state::Ui, ui::widgets::Icon, widget};

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let mut live = ui.live;
    let open = (live.picker)();
    // The widget being picked for, kept while the sheet slides away.
    let mut last = use_signal(|| None::<String>);
    if open.is_some() && *last.peek() != open {
        last.set(open.clone());
    }
    let id = open.clone().or(last());
    let w = id
        .as_ref()
        .and_then(|id| s.widgets.read().iter().find(|w| &w.id == id).cloned());
    let profiles = s.profiles.read().clone();
    rsx! {
        div { class: if open.is_some() { "wscrim on" } else { "wscrim" }, onclick: move |_| live.picker.set(None) }
        div {
            class: if open.is_some() { "wsheet on" } else { "wsheet" },
            role: "dialog",
            "aria-modal": "true",
            "aria-hidden": "{open.is_none()}",
            "aria-labelledby": "wsheettitle",
            div { class: "grab", onclick: move |_| live.picker.set(None) }
            if let Some(w) = w {
                h3 { id: "wsheettitle", "{w.kind.label()} shows" }
                p { class: "why", "One computer per widget. Add another {w.kind.short().to_lowercase()} widget to see a second one." }
                div { class: "list", role: "radiogroup",
                    for (i, p) in profiles.into_iter().enumerate() {
                        {
                            let key = p.key();
                            let on = key == w.host;
                            let r = reach::of(ui, &key);
                            let (wid, k2) = (w.id.clone(), key.clone());
                            rsx! {
                                button {
                                    key: "{key}",
                                    class: if on { "navrow pickrow on" } else { "navrow pickrow" },
                                    role: "radio",
                                    "aria-checked": "{on}",
                                    onclick: move |_| {
                                        widget::rebind(&mut s.widgets.write(), &wid, &k2);
                                        let wid = wid.clone();
                                        spawn(async move {
                                            bridge::sleep(260).await;
                                            live.picker.set(None);
                                            bridge::call(format!("window.__wado.widgetPop({});", bridge::js(&wid)));
                                        });
                                    },
                                    span { class: "disc", style: "--hue:var(--base{profile::hue(i)})", Icon { name: profile::icon(&p) } }
                                    span { class: "navtext",
                                        b { "{p.display()}" }
                                        small { span { class: "lstate {r.tone}", i { class: "ldot" } span { "{r.words}" } } }
                                    }
                                    span { class: "check", Icon { name: "check" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
