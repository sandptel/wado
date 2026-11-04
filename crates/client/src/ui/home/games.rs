//! Games, on home (the app's `ui/play/library.dart`): every app of the selected computer with Game
//! mode on, as a poster — its icon on a wash of one base16 accent slot. One tap opens it as a
//! game ([`game_mode::open`]); Add turns Game mode on for another app, set up once, in place.
//!
//! And the launcher: once the session started for a game shows its first frame, the game's app
//! is launched — unless the app list says it is running already, so a second tap never opens a
//! second window.

use dioxus::prelude::*;

use crate::{
    actions,
    game_mode::{self, Profile},
    state::Ui,
    ui::widgets::Icon,
};

/// The accent slot (08–0F) an app's poster washes in: its name hashed, as the app does.
fn hue(name: &str) -> u32 {
    8 + name
        .bytes()
        .fold(0u32, |a, c| (a.wrapping_mul(31) + u32::from(c)) & 0xffff)
        % 8
}

fn icon_of(ui: Ui, exec: &str) -> Option<String> {
    (ui.live.apps)()
        .into_iter()
        .find(|a| a.exec == exec)
        .and_then(|a| a.icon)
}

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let mut adding = use_signal(|| false);
    let mut setup = use_signal(|| None::<Profile>);

    // The launcher: the game's app, once its session is really up.
    use_effect(move || {
        let up = (live.session_on)() && (live.fps)().is_some();
        let Some(exec) = (live.game_launch)() else {
            if !(live.session_on)() && live.game.peek().is_some() {
                live.game.set(None);
            }
            return;
        };
        if !up {
            return;
        }
        let running = live.apps.peek().iter().any(|a| a.exec == exec && a.running);
        if !running {
            actions::launch_command(ui, &exec);
        }
        live.game_launch.set(None);
    });

    let games = game_mode::on(ui);
    let apps = (live.apps)();
    rsx! {
        div { class: "sect", span { "Games" } }
        div { class: "gshelf", role: "list",
            for p in games.clone() {
                div { key: "{p.exec}", class: "gposter", role: "listitem",
                    button {
                        class: "gart",
                        style: "--h: var(--base{hue(&p.name):02X})",
                        "aria-label": "Play {p.name}",
                        onclick: { let p = p.clone(); move |_| game_mode::open(ui, p.clone()) },
                        if let Some(src) = icon_of(ui, &p.exec) {
                            img { class: "gicon", src: "{src}", alt: "" }
                        } else {
                            span { class: "gletter", "{p.name.chars().next().unwrap_or('?')}" }
                        }
                    }
                    span { class: "gname", "{p.name}" }
                    button {
                        class: "gedit",
                        "aria-label": "Game mode settings for {p.name}",
                        onclick: { let p = p.clone(); move |_| setup.set(Some(p.clone())) },
                        "Game mode"
                    }
                }
            }
            button {
                class: "gposter gadd",
                "aria-expanded": "{adding()}",
                onclick: move |_| {
                    adding.toggle();
                    if adding() && live.apps.peek().is_empty() {
                        crate::bridge::call("window.__wado.requestApps && window.__wado.requestApps();".to_string());
                    }
                },
                span { class: "gart gplus", Icon { name: "plus" } }
                span { class: "gname", if games.is_empty() { "Add a game" } else { "Add" } }
            }
        }
        if adding() && setup().is_none() {
            div { class: "card gpick",
                p { class: "why", "Turn on Game mode for an app you play. Set it up once; it opens that way every time." }
                if apps.is_empty() {
                    p { class: "why", "Asking the computer for its apps…" }
                }
                for a in apps.iter().filter(|a| !a.hidden && !games.iter().any(|g| g.exec == a.exec)).take(40).cloned() {
                    button {
                        key: "{a.exec}",
                        class: "gpickrow",
                        onclick: move |_| setup.set(Some(Profile { name: a.name.clone(), exec: a.exec.clone(), x: game_mode::needs_x(&a.exec), ..Default::default() })),
                        span { "{a.name}" }
                        Icon { name: "plus" }
                    }
                }
            }
        }
        if let Some(p) = setup() {
            {form(ui, p, setup, adding)}
        }
    }
}

/// The setup, in place: the picture, the sound, the X server; Save, or turn it off.
fn form(
    ui: Ui,
    p: Profile,
    mut setup: Signal<Option<Profile>>,
    mut adding: Signal<bool>,
) -> Element {
    let known = game_mode::on(ui).iter().any(|g| g.exec == p.exec);
    rsx! {
        div { class: "card gform",
            div { class: "ghead", "Game mode for {p.name}" }
            p { class: "why", "Every open from this browser uses these. They shape the session only — your own settings stay as they are." }
            div { class: "glabel", "Picture" }
            for (id, title) in [("smooth", "Smoothest"), ("balanced", "Balanced"), ("sharp", "Sharpest")] {
                button {
                    key: "{id}",
                    class: if p.picture == id { "gopt on" } else { "gopt" },
                    "aria-pressed": "{p.picture == id}",
                    onclick: { let p = p.clone(); move |_| setup.set(Some(Profile { picture: id.into(), ..p.clone() })) },
                    b { "{title}" }
                    span { class: "mono", "{game_mode::words(id)}" }
                }
            }
            label { class: "grow",
                input { r#type: "checkbox", checked: p.input, onchange: { let p = p.clone(); move |e: FormEvent| setup.set(Some(Profile { input: e.checked(), ..p.clone() })) } }
                span { "Sound kept tight" }
            }
            label { class: "grow",
                input { r#type: "checkbox", checked: p.x, onchange: { let p = p.clone(); move |e: FormEvent| setup.set(Some(Profile { x: e.checked(), ..p.clone() })) } }
                span { "Older-games mode — Steam and X11-only games need it; their windows can see each other's typing" }
            }
            div { class: "grow2",
                button { class: "btn", onclick: move |_| setup.set(None), "Cancel" }
                if known {
                    button { class: "btn danger", onclick: { let e = p.exec.clone(); move |_| { game_mode::off(ui, &e); setup.set(None); } }, "Turn off" }
                }
                button { class: "btn primary", onclick: { let p = p.clone(); move |_| { game_mode::save(ui, &p); setup.set(None); adding.set(false); } }, "Save" }
            }
        }
    }
}
