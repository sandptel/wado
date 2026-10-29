// SPDX-License-Identifier: AGPL-3.0-only
//! Client mode: other wado computers. The list is this app's own; each computer's relay is
//! asked whether it is up (pushed through `caps::watch::computers`); a computer's page carries
//! its four doors, which open the web client in the browser until native streaming lands
//! (`caps::video`, marked on the page).

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use slint::{ComponentHandle, VecModel};

use crate::{
    App, AppWindow, Client, ComputerRow,
    caps::{
        self,
        computers::{Computer, Seen},
        watch::{self, Producer, Sightings},
    },
    models::{rc, sync},
    theme,
};

pub struct Ctl {
    list: Arc<Mutex<Vec<Computer>>>,
    seen: Sightings,
    producer: Producer<Sightings>,
    model: Rc<VecModel<ComputerRow>>,
}

thread_local! {
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Ctl) -> R) -> Option<R> {
    CTL.with(|c| c.borrow_mut().as_mut().map(f))
}

pub fn install(ui: &AppWindow, live: bool) {
    let mut saved = caps::computers::load();
    // On a host, this computer is in the list from the start — never an empty page.
    if let Some(here) = caps::computers::this_computer() {
        if !saved.iter().any(|c| c.id == here.id) {
            saved.insert(0, here);
        }
    }
    let list = Arc::new(Mutex::new(saved));
    let producer = watch::computers(list.clone(), Duration::from_secs(5));
    let rx = producer.rx.clone();
    let model = Rc::new(VecModel::default());
    let c = ui.global::<Client>();
    c.set_computers(rc(&model));
    c.set_native_video_why(caps::video::native().why().unwrap_or_default().into());
    CTL.with(|c| {
        *c.borrow_mut() = Some(Ctl {
            list,
            seen: Vec::new(),
            producer,
            model,
        })
    });
    wire(ui);
    show(ui);
    if live {
        crate::bridge::subscribe(rx, ui.as_weak(), apply);
    }
}

/// What the relays said, applied.
pub fn apply(ui: &AppWindow, seen: Sightings) {
    with(|c| c.seen = seen);
    show(ui);
}

fn show(ui: &AppWindow) {
    let here = caps::computers::this_computer().map(|c| c.id);
    with(|c| {
        let list = c.list.lock().map(|l| l.clone()).unwrap_or_default();
        let rows = list
            .iter()
            .enumerate()
            .map(|(i, comp)| {
                let seen = c
                    .seen
                    .iter()
                    .find(|(id, _)| *id == comp.id)
                    .map_or(Seen::Checking, |(_, s)| *s);
                row(ui, i, comp, seen, here.as_deref() == Some(comp.id.as_str()))
            })
            .collect();
        sync(&c.model, rows);
    });
}

fn row(ui: &AppWindow, i: usize, c: &Computer, seen: Seen, here: bool) -> ComputerRow {
    let (words, detail, tone, motion, online, n) = match seen {
        Seen::Checking => ("Checking…", "asking its relay", "dim", "breathe", false, 0),
        Seen::Online(n) => ("Online", "its relay sees it", "good", "ripple", true, n),
        Seen::Offline => (
            "Offline",
            "its relay has not seen it",
            "bad",
            "still",
            false,
            0,
        ),
        Seen::NoRelay => (
            "Relay unreachable",
            "not the computer's fault",
            "caution",
            "breathe",
            false,
            0,
        ),
    };
    let host = c
        .relay
        .split("://")
        .nth(1)
        .unwrap_or(&c.relay)
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();
    ComputerRow {
        id: c.id.as_str().into(),
        pretty_id: caps::computers::pretty(&c.id).into(),
        name: c.name.as_str().into(),
        letters: crate::host::initials(&c.name).into(),
        hue: theme::hue(ui, i),
        relay: c.relay.as_str().into(),
        relay_host: host.into(),
        status_words: words.into(),
        status_detail: detail.into(),
        status_tone: tone.into(),
        status_motion: motion.into(),
        online,
        daemons: n as i32,
        here,
    }
}

fn save_and_show(ui: &AppWindow) {
    let r = with(|c| {
        let l = c.list.lock().map(|l| l.clone()).unwrap_or_default();
        c.producer.poke();
        caps::computers::save(&l)
    });
    if let Some(Err(e)) = r {
        crate::toast::say(ui, &format!("Could not save the list: {e}"), "bad");
    }
    show(ui);
}

fn wire(ui: &AppWindow) {
    let g = ui.global::<Client>();
    let weak = ui.as_weak();
    g.on_open(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        ui.global::<Client>().set_current(i);
        let a = ui.global::<App>();
        a.set_mode(1);
        a.set_client_page(1);
    });
    let weak = ui.as_weak();
    g.on_add(move |name, id, relay| {
        let Some(ui) = weak.upgrade() else { return };
        match caps::computers::parse(&name, &id, &relay) {
            Ok(comp) => {
                let dup = with(|c| {
                    let mut l = c.list.lock().unwrap_or_else(|e| e.into_inner());
                    if l.iter().any(|x| x.id == comp.id) {
                        return true;
                    }
                    l.push(comp.clone());
                    false
                })
                .unwrap_or(false);
                if dup {
                    ui.global::<Client>()
                        .set_add_error("That computer is already in the list.".into());
                    return;
                }
                ui.global::<Client>().set_add_error("".into());
                crate::toast::say(&ui, &format!("Added {}", comp.name), "good");
                save_and_show(&ui);
            }
            Err(e) => ui.global::<Client>().set_add_error(e.into()),
        }
    });
    let weak = ui.as_weak();
    g.on_forget(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        let name = with(|c| {
            let mut l = c.list.lock().unwrap_or_else(|e| e.into_inner());
            ((i as usize) < l.len()).then(|| l.remove(i as usize).name)
        })
        .flatten();
        if let Some(n) = name {
            crate::toast::say(
                &ui,
                &format!("Forgot {n} — it is only gone from this app"),
                "good",
            );
        }
        ui.global::<Client>().set_current(-1);
        ui.global::<App>().set_client_page(0);
        save_and_show(&ui);
    });
    let weak = ui.as_weak();
    g.on_rename(move |i, name| {
        let Some(ui) = weak.upgrade() else { return };
        with(|c| {
            let mut l = c.list.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(x) = l.get_mut(i as usize) {
                x.name = name.trim().to_string();
            }
        });
        save_and_show(&ui);
    });
    g.on_refresh(move || {
        with(|c| c.producer.poke());
    });
    let weak = ui.as_weak();
    g.on_launch(move |i, _door| {
        let Some(ui) = weak.upgrade() else { return };
        let comp = with(|c| c.list.lock().ok().and_then(|l| l.get(i as usize).cloned())).flatten();
        let Some(comp) = comp else { return };
        // Native streaming is absent in this build (`caps::video`); the web client does the job.
        let url = caps::computers::web_link(&comp);
        match caps::desktop::open_url(&url) {
            Ok(()) => crate::toast::say(
                &ui,
                &format!("Opening {} in your browser", comp.name),
                "good",
            ),
            Err(e) => crate::toast::say(&ui, &e, "bad"),
        }
    });
}

/// Ask every relay now, on this thread — for the screenshot mode.
pub fn refresh_now(ui: &AppWindow) {
    let list = with(|c| c.list.lock().map(|l| l.clone()).unwrap_or_default()).unwrap_or_default();
    let seen: Sightings = list
        .iter()
        .map(|c| (c.id.clone(), caps::computers::probe(c)))
        .collect();
    apply(ui, seen);
}
