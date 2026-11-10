//! The four reasons to open a computer: its desktop, this phone as its input (the Virtual input
//! tile, `input/tile.rs`), its files, a shell. Big tiles, Desktop first and loudest — it is the one most taps are for.
//!
//! Each says what it will do *now* ("Rejoin · Firefox and 2 more", "Start at this screen's
//! size"), and a door this phone may not use says so instead of failing after the tap.

use dioxus::prelude::*;

use crate::{
    bridge,
    host_caps::Cap,
    reach::Reach,
    state::Ui,
    ui::{home::sessions, widgets::Icon},
};

/// Which doors this phone may use there. Shared with the app icon's shortcuts
/// ([`crate::deeplink`]), so a shortcut never opens a door this page shows shut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Doors {
    pub pad: bool,
    pub files: bool,
    pub shell: bool,
}

pub fn doors(r: &Reach) -> Doors {
    Doors {
        pad: r.may("input"),
        files: r.grants().as_ref().is_none_or(|g| g.files != "none"),
        shell: r.may("shells") && r.config.as_ref().is_none_or(|c| c.shells),
    }
}

/// The Desktop door: take over a computer in use elsewhere, rejoin what runs, or start anew.
pub fn desktop(ui: Ui, r: &Reach) {
    let running = !(ui.live.sessions)().1.is_empty();
    if r.busy && r.running_there == Some(false) {
        // The other device holds the seat with nothing running: one tap moves it here *and*
        // starts (`relayConnect` takes the seat, then asks for a session).
        crate::actions::start(ui);
    } else if r.busy {
        bridge::call("window.__wado.relayTakeover();".to_string());
    } else if running {
        sessions::join(ui);
    } else {
        crate::actions::start(ui);
    }
}

/// What an offline door says under its name.
const OFFLINE: &str = "Offline · opens when it is back";

#[component]
pub fn Actions(ui: Ui, r: Reach, name: String) -> Element {
    let live = ui.live;
    let relay = (ui.set.conn_mode)() == "relay";
    let (_, list) = (live.sessions)();
    let running = !list.is_empty();
    let shells_open = (live.shells)().iter().filter(|s| s.alive).count();
    let off = !r.online;
    // A door this phone may not use answers its tap with why, and the fix (`grant::Grant`).
    let mut why = use_signal(|| None::<(&'static str, &'static str)>);
    let Doors {
        files: files_ok,
        shell: shell_ok,
        ..
    } = doors(&r);

    let idle_elsewhere = r.busy && r.running_there == Some(false);
    let desk_sub = if idle_elsewhere {
        let who = if r.holder.is_empty() {
            "your other device"
        } else {
            &r.holder
        };
        format!("Use it here — {who} is connected, nothing running")
    } else if r.busy {
        "In use on another device".to_string()
    } else if running {
        let apps: Vec<String> = list
            .iter()
            .flat_map(|s| s.apps.iter())
            .map(|a| crate::ui::home::sessions::short(a))
            .collect();
        match apps.len() {
            0 => "Rejoin · empty desktop".into(),
            1 => format!("Rejoin · {}", apps[0]),
            n => format!("Rejoin · {} and {} more", apps[0], n - 1),
        }
    } else {
        "Start at this screen's size".into()
    };

    // No session on this computer yet (`crate::host_caps`): the door stays, dimmed, and says why.
    let session = r.cap("session");
    let later = matches!(session, Cap::Later(_));
    // Offline: each door stays, dimmed, says so, and a tap says when — never a dead button.
    let desk_off = off && !r.busy;
    let desk_sub = if later {
        session.why().to_string()
    } else if desk_off {
        OFFLINE.to_string()
    } else {
        desk_sub
    };
    let tell_off = move |what: &str| {
        crate::host_caps::tell(
            ui,
            &format!("Offline — {what} opens once the computer is back."),
        )
    };

    rsx! {
        section { class: "acts sx o2", style: "--n:1", "aria-label": "Open",
            if !matches!(session, Cap::Off(_)) {
                button {
                    class: if later { "act primary soon" } else if desk_off { "act primary off" } else { "act primary" },
                    "aria-disabled": desk_off,
                    onclick: {
                        let r = r.clone();
                        let why = session.why().to_string();
                        move |_| if later { crate::host_caps::tell(ui, &why) } else if desk_off { tell_off("Desktop") } else { desktop(ui, &r) }
                    },
                    if later { span { class: "badge", "{crate::host_caps::SOON}" } }
                    if running && !r.busy && !later && !desk_off { span { class: "badge", "running" } }
                    if r.busy && !idle_elsewhere && !later { span { class: "badge", "take over" } }
                    span { class: "aic", Icon { name: "play", fill: true } }
                    span { b { "Desktop" } small { "{desk_sub}" } }
                }
            }
            crate::ui::computer::input::tile::Tile { ui, r: r.clone() }
            button {
                class: if off { "act off" } else { "act" },
                "aria-disabled": off,
                disabled: !relay,
                onclick: move |_| if off { tell_off("Files") } else if files_ok { bridge::call("window.__wado.filesOpen();".to_string()) } else { why.set(Some(("files-ro", "its files"))) },
                span { class: "aic", Icon { name: "folder" } }
                span { b { "Files" } small { if off { "{OFFLINE}" } else if files_ok { "Browse, send, download" } else { "Not allowed for this phone · why?" } } }
            }
            button {
                class: if off { "act off" } else { "act" },
                "aria-disabled": off,
                disabled: !relay,
                onclick: move |_| if off { tell_off("Shell") } else if shell_ok { crate::ui::console::open_shells(ui) } else { why.set(Some(("shells", "its shells"))) },
                if shells_open > 0 && !off { span { class: "badge", "{shells_open} open" } }
                span { class: "aic", Icon { name: "term" } }
                span { b { "Shell" } small { if off { "{OFFLINE}" } else if shell_ok { "A terminal, no video" } else { "Not allowed for this phone · why?" } } }
            }
        }
        if let Some((token, what)) = why() {
            {
                let g = r.grants().unwrap_or_default();
                rsx! {
                    crate::ui::computer::grant::Grant {
                        computer: name.clone(),
                        device: g.name.clone(),
                        token: token.to_string(),
                        what: what.to_string(),
                        has: crate::ui::computer::grant::has_words(&g),
                        title: format!("{} is off for this phone", if token == "shells" { "Shell" } else { "Files" }),
                    }
                }
            }
        }
    }
}
