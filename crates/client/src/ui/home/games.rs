//! Home's play section (the app's `ui/play/library.dart`, same shared rules — `wado_ux`): played
//! lately → "Jump back in", a carousel of recent games; otherwise one Continue card for the last
//! thing done. Then Games — every app with Game mode on, as a poster (its icon on a wash of one
//! base16 accent slot). All computers: what was left on each, then each one's games.
//!
//! Per form: touch shows a poster's "Game mode" under it and opens it on a long press; a mouse
//! shows it on hover and focus, and the posters lift. The setup is the shared sheet (`ui::sheet`).
//!
//! And the launcher: once the session started for a game shows its first frame, the game's app is
//! launched — unless the app list says it is running already, so a second tap never opens two.

use dioxus::prelude::*;
use wado_protocol::AppEntry;

use crate::{
    actions, bridge,
    game_mode::{self, Profile},
    recent,
    state::Ui,
    ui::{sheet::Sheet, widgets::Icon},
};

fn hue(name: &str) -> String {
    format!("{:02X}", wado_ux::poster::hue(name))
}

fn icon_of(apps: &[AppEntry], exec: &str) -> Option<String> {
    apps.iter()
        .find(|a| a.exec == exec)
        .and_then(|a| a.icon.clone())
}

/// Grow the poster into the game (`js/launch.js`), then open it.
fn play(ui: Ui, p: Profile, from: &str) {
    let mut live = ui.live;
    live.launching.set(Some((p.name.clone(), hue(&p.name))));
    bridge::call(format!("window.__wado.launchGrow({});", bridge::js(&from)));
    game_mode::open(ui, p);
}

pub fn render(ui: Ui) -> Element {
    let mut live = ui.live;
    let setup = use_signal(|| None::<(Profile, bool)>);
    let mut picking = use_signal(|| false);

    // The launcher: the game's app, once its session is really up; the shadow ends with it.
    use_effect(move || {
        let on = (live.session_on)();
        let up = on && (live.fps)().is_some();
        if !on {
            game_mode::end(ui);
        }
        let Some(exec) = (live.game_launch)() else {
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

    let apps = (live.apps)();
    let all = (ui.set.home_all)() && ui.set.profiles.read().len() > 1;
    let here = game_mode::computer(ui);
    let list = recent::list(ui);
    let now = crate::profile::now_s();

    rsx! {
        if all {
            {everything(ui, &list, now)}
        } else {
            {lately(ui, &list, &here, now, &apps)}
            div { class: "sect", span { "Games" } }
            {shelf(ui, game_mode::on(ui), &apps, setup, picking)}
        }
        if picking() {
            {pick(ui, &apps, setup, picking)}
        }
        if let Some((p, known)) = setup() {
            {form(ui, p, known, setup)}
        }
    }
}

/// Jump back in (games, played lately) or Continue (the last thing, any kind).
fn lately(ui: Ui, list: &[recent::Recent], here: &str, now: u64, apps: &[AppEntry]) -> Element {
    let mine: Vec<&recent::Recent> = list.iter().filter(|r| r.computer == here).collect();
    let games: Vec<Profile> = mine
        .iter()
        .filter(|r| r.kind == "game")
        .filter_map(|r| game_mode::on(ui).into_iter().find(|p| p.exec == r.exec))
        .collect();
    if recent::gamed_lately(list, here, now) && !games.is_empty() {
        return rsx! {
            div { class: "sect", span { "Jump back in" } }
            div { class: "gcarousel", role: "list",
                for p in games {
                    {wide(ui, p, apps)}
                }
            }
        };
    }
    let Some(r) = mine.first().map(|r| (*r).clone()) else {
        return rsx! {};
    };
    let p = game_mode::on(ui)
        .into_iter()
        .find(|p| r.kind == "game" && p.exec == r.exec);
    let kind = match r.kind.as_str() {
        "game" => "Game mode",
        "shell" => "Shell",
        "file" => "File",
        _ => "Desktop",
    };
    rsx! {
        div { class: "sect", span { "Continue" } }
        button {
            class: "gcont",
            "data-poster": "continue",
            style: "--h: var(--base{hue(&r.name)})",
            onclick: move |_| match p.clone() {
                Some(p) => play(ui, p, "continue"),
                None => crate::actions::start(ui),
            },
            span { class: "gletter", "{r.name.chars().next().unwrap_or('?')}" }
            span { class: "gcontw", b { "{r.name}" } small { "{kind} · {recent::ago(r.at, now)}" } }
            span { class: "gplay", "aria-hidden": "true", Icon { name: "play", fill: true } }
        }
    }
}

fn wide(ui: Ui, p: Profile, apps: &[AppEntry]) -> Element {
    let a = apps.iter().find(|a| a.exec == p.exec);
    let running = a.is_some_and(|a| a.running);
    let icon = a.and_then(|a| a.icon.clone());
    let key = format!("w-{}", p.exec);
    rsx! {
        button {
            key: "{p.exec}",
            class: "gwide",
            role: "listitem",
            "data-poster": "{key}",
            style: "--h: var(--base{hue(&p.name)})",
            "aria-label": if running { "{p.name}, still running" } else { "Play {p.name}" },
            onclick: { let p = p.clone(); move |_| play(ui, p.clone(), &key) },
            span { class: if running { "gtag live" } else { "gtag" }, if running { "Still running" } else { "Game mode" } }
            b { "{p.name}" }
            small { "{game_mode::play_words(&p.play)}" }
            if let Some(src) = icon { img { class: "gicon", src: "{src}", alt: "" } }
            span { class: "gplay", "aria-hidden": "true", Icon { name: "play", fill: true } }
        }
    }
}

fn shelf(
    ui: Ui,
    games: Vec<Profile>,
    apps: &[AppEntry],
    mut setup: Signal<Option<(Profile, bool)>>,
    mut picking: Signal<bool>,
) -> Element {
    rsx! {
        div { class: "gshelf", role: "list",
            for p in games.clone() {
                {poster(ui, p, apps, setup)}
            }
            button {
                class: "gposter gadd",
                onclick: move |_| {
                    picking.set(true);
                    setup.set(None);
                    bridge::call("window.__wado.requestApps && window.__wado.requestApps();".to_string());
                },
                span { class: "gart gplus", Icon { name: "plus" } }
                span { class: "gname", if games.is_empty() { "Add a game" } else { "Add" } }
                if games.is_empty() { small { class: "gsub", "set up once" } }
            }
        }
    }
}

fn poster(
    ui: Ui,
    p: Profile,
    apps: &[AppEntry],
    mut setup: Signal<Option<(Profile, bool)>>,
) -> Element {
    let a = apps.iter().find(|a| a.exec == p.exec);
    let running = a.is_some_and(|a| a.running);
    let icon = icon_of(apps, &p.exec);
    let key = format!("p-{}", p.exec);
    rsx! {
        div { key: "{p.exec}", class: "gposter", role: "listitem",
            button {
                class: "gart",
                "data-poster": "{key}",
                style: "--h: var(--base{hue(&p.name)})",
                "aria-label": if running { "{p.name}, running. Play" } else { "Play {p.name}" },
                onclick: { let p = p.clone(); let k = key.clone(); move |_| play(ui, p.clone(), &k) },
                // A long press on touch (the browser's context menu) opens its Game mode.
                oncontextmenu: { let p = p.clone(); move |e: MouseEvent| { e.prevent_default(); setup.set(Some((p.clone(), true))); } },
                if running { span { class: "glive", "aria-hidden": "true" } }
                if let Some(src) = icon { img { class: "gicon", src: "{src}", alt: "" } }
                else { span { class: "gletter", "{p.name.chars().next().unwrap_or('?')}" } }
            }
            span { class: "gname", "{p.name}" }
            button {
                class: "gedit",
                "aria-label": "Game mode settings for {p.name}",
                onclick: { let p = p.clone(); move |_| setup.set(Some((p.clone(), true))) },
                "{game_mode::play_words(&p.play)}"
            }
        }
    }
}

/// All computers: what was left on each, newest first, then each one's games.
fn everything(ui: Ui, list: &[recent::Recent], now: u64) -> Element {
    let comps = ui.set.profiles.read().clone();
    let name_of = |key: &str| {
        comps
            .iter()
            .find(|c| c.key() == key)
            .map(|c| c.display())
            .unwrap_or_default()
    };
    let left: Vec<recent::Recent> = list
        .iter()
        .filter(|r| comps.iter().any(|c| c.key() == r.computer))
        .take(4)
        .cloned()
        .collect();
    rsx! {
        if !left.is_empty() {
            div { class: "sect", span { "Pick up where you left off" } }
            div { class: "gleft",
                for r in left {
                    div { key: "{r.computer}{r.exec}{r.kind}", class: "gleftrow", style: "--h: var(--base{hue(&r.name)})",
                        span { class: "gletter", "{r.name.chars().next().unwrap_or('?')}" }
                        span { class: "gcontw", b { "{r.name}" } small { "{name_of(&r.computer)} · {recent::ago(r.at, now)}" } }
                    }
                }
            }
        }
        for c in comps.iter() {
            if !game_mode::on_for(ui, &c.key()).is_empty() {
                div { key: "{c.key()}", class: "sect", span { "Games · {c.display()}" } }
                div { class: "gshelf", role: "list",
                    for p in game_mode::on_for(ui, &c.key()) {
                        div { key: "{c.key()}{p.exec}", class: "gposter", role: "listitem",
                            span { class: "gart", style: "--h: var(--base{hue(&p.name)})",
                                span { class: "gletter", "{p.name.chars().next().unwrap_or('?')}" }
                            }
                            span { class: "gname", "{p.name}" }
                        }
                    }
                }
            }
        }
    }
}

/// Pick the app to turn Game mode on for.
fn pick(
    ui: Ui,
    apps: &[AppEntry],
    mut setup: Signal<Option<(Profile, bool)>>,
    mut picking: Signal<bool>,
) -> Element {
    let on = game_mode::on(ui);
    let mut list: Vec<AppEntry> = apps
        .iter()
        .filter(|a| !a.hidden && !on.iter().any(|g| g.exec == a.exec))
        .take(60)
        .cloned()
        .collect();
    list.sort_by_key(|a| !game_mode::needs_x(&a.exec));
    rsx! {
        Sheet {
            title: "Turn on Game mode".to_string(),
            sub: Some("For any app you play. Set it up once; it opens that way every time.".to_string()),
            on_close: move |_| picking.set(false),
            if apps.is_empty() { p { class: "why", "Asking the computer for its apps…" } }
            div { class: "gpicklist",
                for a in list {
                    button {
                        key: "{a.exec}",
                        class: "gpickrow",
                        onclick: move |_| { picking.set(false); setup.set(Some((game_mode::new(&a.name, &a.exec), false))); },
                        if let Some(src) = a.icon.clone() { img { class: "gpickicon", src: "{src}", alt: "" } }
                        span { "{a.name}" }
                        Icon { name: "plus" }
                    }
                }
            }
        }
    }
}

/// The setup: how it is played, then the session's rules — the app's two steps, on one sheet.
fn form(ui: Ui, p: Profile, known: bool, mut setup: Signal<Option<(Profile, bool)>>) -> Element {
    let layouts = crate::ui::computer::input::prefs::GAMEPAD
        .read()
        .list
        .clone();
    let mut set = move |q: Profile| setup.set(Some((q, known)));
    rsx! {
        Sheet {
            title: format!("Game mode for {}", p.name),
            sub: Some("Every open from this browser uses these. They shape the session only — your own settings stay as they are.".to_string()),
            on_close: move |_| setup.set(None),
            div { class: "glabel", "How you play" }
            for (id, title, sub) in [("pad", "On-screen pad", "Buttons over the game"), ("controller", "My controller", "A gamepad plugged into this device"), ("touch", "Touch & mouse", "Like the desktop")] {
                button {
                    key: "{id}",
                    class: if p.play == id { "gopt on" } else { "gopt" },
                    "aria-pressed": "{p.play == id}",
                    onclick: { let p = p.clone(); move |_| set(Profile { play: id.into(), ..p.clone() }) },
                    b { "{title}" }
                    span { "{sub}" }
                }
            }
            if p.play == "pad" && !layouts.is_empty() {
                div { class: "gchips",
                    for (id, name) in layouts {
                        button {
                            key: "{id}",
                            class: if p.layout == id { "gchip on" } else { "gchip" },
                            onclick: { let p = p.clone(); let id = id.clone(); move |_| set(Profile { layout: id.clone(), ..p.clone() }) },
                            "{name}"
                        }
                    }
                }
            }
            div { class: "glabel", "Picture" }
            for (id, title) in [("smooth", "Smoothest"), ("balanced", "Balanced"), ("sharp", "Sharpest")] {
                button {
                    key: "{id}",
                    class: if p.picture == id { "gopt on" } else { "gopt" },
                    "aria-pressed": "{p.picture == id}",
                    onclick: { let p = p.clone(); move |_| set(Profile { picture: id.into(), ..p.clone() }) },
                    b { "{title}" }
                    span { class: "mono", "{game_mode::picture_words(id, 120)}" }
                }
            }
            label { class: "grow",
                input { r#type: "checkbox", checked: p.input, onchange: { let p = p.clone(); move |e: FormEvent| set(Profile { input: e.checked(), ..p.clone() }) } }
                span { "Never miss a press — buttons sent twice, sound kept tight" }
            }
            label { class: "grow",
                input { r#type: "checkbox", checked: p.calm, onchange: { let p = p.clone(); move |e: FormEvent| set(Profile { calm: e.checked(), ..p.clone() }) } }
                span { "No stray gestures while playing" }
            }
            label { class: "grow",
                input { r#type: "checkbox", checked: p.x, onchange: { let p = p.clone(); move |e: FormEvent| set(Profile { x: e.checked(), ..p.clone() }) } }
                span { "Older-games mode — Steam and X11-only games need it; their windows can see each other's typing" }
            }
            div { class: "grow2",
                if known {
                    button { class: "btn danger", onclick: { let e = p.exec.clone(); move |_| { game_mode::off(ui, &e); setup.set(None); } }, "Turn off" }
                }
                button { class: "btn primary", onclick: { let p = p.clone(); move |_| { game_mode::save(ui, &p); setup.set(None); if !known { play(ui, p.clone(), ""); } } }, if known { "Save" } else { "Save and play" } }
            }
        }
    }
}
