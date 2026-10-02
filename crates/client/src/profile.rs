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
    pub name: String,
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
        Some(p) => *p = capture(ui, p.name.clone()),
        None => {
            list.push(capture(ui, default_name(ui)));
            drop(list);
            s.profile.set(s.profiles.read().len() - 1);
        }
    }
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
