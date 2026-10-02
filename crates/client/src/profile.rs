//! Saved hosts. Each profile is a connection *and* the session shape last used with it, so
//! switching from the phone-sized workstation to the 4K desktop restores both at once.
//!
//! Client-side by necessity: the list is what tells the client which daemon to reach, so it
//! cannot live on a daemon.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::state::Ui;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// What the person called it. Empty: it is named after the computer (`host`).
    pub name: String,
    /// The computer's hostname, as it last said — the name until the person picks one.
    pub host: String,
    pub conn_mode: String,
    pub server_addr: String,
    pub relay_url: String,
    pub remote_id: String,
    pub res: String,
    pub fps: u32,
    pub scale: String,
    pub quality: String,
}

impl Profile {
    /// What the card says: the person's name for it, else the computer's own, else where it is.
    pub fn display(&self) -> String {
        [&self.name, &self.host]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| self.address())
    }

    /// Where its relay and Remote ID are — the key its live status is filed under.
    pub fn key(&self) -> String {
        format!("{}|{}", self.relay_url, self.remote_id)
    }

    /// A one-line address for the card.
    pub fn address(&self) -> String {
        if self.conn_mode == "relay" {
            if self.remote_id.trim().is_empty() {
                "relay · no Remote ID yet".to_string()
            } else {
                format!("relay · id {}", self.remote_id)
            }
        } else {
            format!(
                "direct · {}",
                self.server_addr.trim_start_matches("http://")
            )
        }
    }
}

/// The settings as they stand, as a profile named `name`.
pub fn capture(ui: Ui, name: String) -> Profile {
    let s = ui.set;
    Profile {
        name,
        host: String::new(),
        conn_mode: (s.conn_mode)(),
        server_addr: (s.server_addr)(),
        relay_url: (s.relay_url)(),
        remote_id: (s.remote_id)(),
        res: (s.res)(),
        fps: (s.fps)(),
        scale: (s.scale)(),
        quality: (s.quality)(),
    }
}

/// Make profile `i` current: copy it into the settings.
pub fn select(ui: Ui, i: usize) {
    let mut s = ui.set;
    let Some(p) = s.profiles.read().get(i).cloned() else {
        return;
    };
    s.profile.set(i);
    // Another computer's state is not this one's.
    let mut live = ui.live;
    live.hoststate.set(None);
    if p.conn_mode == "relay" {
        crate::bridge::call(format!(
            "window.__wado.relayDial({}, {});",
            crate::bridge::js(&p.relay_url),
            crate::bridge::js(&p.remote_id)
        ));
    }
    s.conn_mode.set(p.conn_mode);
    s.server_addr.set(p.server_addr);
    s.relay_url.set(p.relay_url);
    s.remote_id.set(p.remote_id);
    // An empty shape means "never started" — leave the device default the effect in main chose.
    if !p.res.is_empty() {
        s.res.set(p.res);
        s.fps.set(p.fps);
        s.scale.set(p.scale);
        s.quality.set(p.quality);
    }
}

/// Write the current settings back into the selected profile — called on Start, so a profile
/// remembers the shape it was last used at. With no profiles yet, the first one is made here.
pub fn remember(ui: Ui) {
    let mut s = ui.set;
    let i = (s.profile)();
    let mut list = s.profiles.write();
    match list.get_mut(i) {
        Some(p) => {
            let host = p.host.clone();
            *p = capture(ui, p.name.clone());
            p.host = host;
        }
        None => {
            list.push(capture(ui, default_name(ui)));
            drop(list);
            s.profile.set(s.profiles.read().len() - 1);
        }
    }
}

/// The computer said its hostname: file it on the selected profile — making one if there is
/// none yet — and drop a name that was only ever a placeholder, so the hostname shows.
pub fn learn_host(ui: Ui, hostname: &str) {
    if hostname.is_empty() {
        return;
    }
    let mut s = ui.set;
    let i = (s.profile)();
    if s.profiles.read().get(i).is_none() {
        let p = capture(ui, String::new());
        s.profiles.write().push(p);
        s.profile.set(s.profiles.read().len() - 1);
    }
    let i = (s.profile)();
    let needs = s
        .profiles
        .read()
        .get(i)
        .is_some_and(|p| p.host != hostname || placeholder(&p.name));
    if needs {
        if let Some(p) = s.profiles.write().get_mut(i) {
            p.host = hostname.to_string();
            if placeholder(&p.name) {
                p.name = String::new();
            }
        }
    }
}

/// A name wado made up rather than one the person typed.
fn placeholder(name: &str) -> bool {
    name == "My computer" || name == "This network" || name.starts_with("Computer ")
}

pub fn default_name(ui: Ui) -> String {
    let s = ui.set;
    if (s.conn_mode)() == "relay" {
        let id = (s.remote_id)();
        if id.is_empty() {
            "My computer".into()
        } else {
            format!("Computer {id}")
        }
    } else {
        "This network".into()
    }
}
