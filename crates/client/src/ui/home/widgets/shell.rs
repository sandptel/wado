//! One widget's card: its header — what it is, and a chip naming the computer it follows (its
//! status dot, its name; a tap opens that computer's page, or in edit mode picks another) — and
//! its body, which is the kind's own file. In edit mode the body folds away and the header grows
//! remove, ↑ ↓ and a drag grip.
//!
//! A widget is never hidden for a computer that is away or a grant that is missing: it says so,
//! in the body, with what would fix it.

use dioxus::prelude::*;

use crate::{
    bridge, reach, route,
    state::Ui,
    ui::widgets::Icon,
    widget::{self, Need, Widget, WidgetKind},
};

use super::{brightness, claude, clipboard, media, trackpad, volume};

/// What every body is handed: the widget, and the computer it follows as this page knows it now.
#[derive(Clone, PartialEq)]
pub struct Ctx {
    pub ui: Ui,
    pub w: Widget,
    pub name: String,
    pub r: reach::Reach,
}

#[component]
pub fn Card(ui: Ui, w: Widget, k: usize, n: usize, editing: bool) -> Element {
    let pk = w.host.clone();
    let r = reach::of(ui, &pk);
    let idx = route::index_of(ui, &pk);
    let (name, hue) = match idx {
        Some(i) => (ui.set.profiles.read()[i].display(), crate::profile::hue(i)),
        None => ("A forgotten computer".to_string(), "03"),
    };
    let away = !r.online && r.tone == "bad";
    let kind = w.kind;
    let id = w.id.clone();
    let (id1, id2, id3, id4, id5) = (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
    let pk2 = pk.clone();
    let mut live = ui.live;
    let cx = Ctx {
        ui,
        w: w.clone(),
        name: name.clone(),
        r: r.clone(),
    };

    rsx! {
        div {
            class: if away { "widget away" } else { "widget" },
            "data-id": "{id}",
            style: "--k:{k};--hue:var(--base{hue})",
            div { class: "whead",
                if editing {
                    button { class: "wrm", "aria-label": "Remove the {kind.short()} widget",
                        onclick: move |_| bridge::call(format!("window.__wado.widgetGone({});", bridge::js(&id1))),
                        span { Icon { name: "minus" } }
                    }
                    span { class: "wico", Icon { name: kind.icon() } }
                } else {
                    Icon { name: kind.icon() }
                }
                span { class: "wname",
                    // The Clipboard widget's title opens the full clipboard rail.
                    if kind == WidgetKind::Clipboard && !editing {
                        button { class: "wtitle linky", onclick: move |_| clipboard::open_rail(ui, &pk2), "{kind.label()}" }
                    } else {
                        span { class: "wtitle", "{kind.label()}" }
                    }
                    button {
                        class: "wfor",
                        "aria-label": if editing { "{kind.short()} shows {name} — change computer" } else { "{name} — open its page" },
                        onclick: move |_| if editing { live.picker.set(Some(id2.clone())) } else { route::open_computer(ui, &pk) },
                        span { class: "lstate {r.tone}", i { class: "ldot" } }
                        "{name}"
                    }
                }
                if editing {
                    span { class: "wmove",
                        button { class: "iconbtn", "aria-label": "Move up", disabled: k == 0,
                            onclick: move |_| step(ui, &id3, -1), Icon { name: "up" } }
                        button { class: "iconbtn", "aria-label": "Move down", disabled: k + 1 == n,
                            onclick: move |_| step(ui, &id4, 1), Icon { name: "down" } }
                        span { class: "grip", role: "button", tabindex: "0", "aria-label": "Drag to reorder — or arrow keys",
                            onkeydown: move |e: KeyboardEvent| match e.key() {
                                Key::ArrowUp => { e.prevent_default(); step(ui, &id5, -1); }
                                Key::ArrowDown => { e.prevent_default(); step(ui, &id5, 1); }
                                _ => {}
                            },
                            Icon { name: "grip" }
                        }
                    }
                }
            }
            if !editing {
                div { class: "wbody", {body(cx, away, hue)} }
            }
        }
    }
}

fn step(ui: Ui, id: &str, by: i32) {
    let mut s = ui.set;
    widget::step(&mut s.widgets.write(), id, by);
}

fn body(cx: Ctx, away: bool, _hue: &str) -> Element {
    let r = &cx.r;
    let name = cx.name.clone();
    if away {
        let seen = if r.seen.is_empty() {
            String::new()
        } else {
            format!(" Seen {}.", r.seen)
        };
        return rsx! {
            div { class: "wnote", Icon { name: "alert" } span { b { "{name} is offline." } "{seen} This fills in when it is back." } }
        };
    }
    // Online, but no seatless link to it: say what is missing rather than take a seat.
    if !r.nolive.is_empty() {
        let why = if r.nolive == "old_relay" {
            format!("Live values for {name} need a newer relay.")
        } else {
            format!("Update wado on {name} to see live values here.")
        };
        return rsx! {
            div { class: "wnote quiet", Icon { name: "info" } span { "{why} Open its page to control it." } }
        };
    }
    let need = cx.w.kind.needs();
    if !r.may(need.token()) {
        let what = match (cx.w.kind, need) {
            (_, Need::Clipboard) => format!("see {name}'s clipboard"),
            (_, Need::Input) => format!("use {name} as a trackpad"),
            (_, Need::Shells) => format!("see Claude on {name}"),
            (WidgetKind::Media, _) => format!("control {name}'s media"),
            (WidgetKind::Brightness, _) => format!("set {name}'s brightness"),
            _ => format!("control {name}'s volume"),
        };
        return rsx! {
            div { class: "wnote", Icon { name: "lock" } span { "This phone may not {what}. Open its page to see how to allow it." } }
        };
    }
    match cx.w.kind {
        WidgetKind::Media => rsx! { media::Media { cx } },
        WidgetKind::Volume => rsx! { volume::Volume { cx } },
        WidgetKind::Brightness => rsx! { brightness::Brightness { cx } },
        WidgetKind::Clipboard => rsx! { clipboard::Clipboard { cx } },
        WidgetKind::Trackpad => rsx! { trackpad::Trackpad { cx } },
        WidgetKind::Claude => rsx! { claude::Claude { cx } },
    }
}

/// "Asking X…" — the computer is there but has not said its state yet.
pub fn asking(name: &str) -> Element {
    rsx! { div { class: "wnote quiet", span { class: "wspin" } "Asking {name}…" } }
}
