// SPDX-License-Identifier: AGPL-3.0-only
//! Devices: the trust list as cards, each grant a toggle. Every change is `wado allow|deny`.

use slint::ComponentHandle;

use super::Ctl;
use crate::{
    AppWindow, DeviceRow, Grants, Host,
    caps::{self, devices::Row, watch::HostSnapshot},
    models::sync,
    theme,
};

pub fn wire(ui: &AppWindow) {
    let h = ui.global::<Host>();
    let weak = ui.as_weak();
    h.on_set_grant(move |key, token, on| {
        let Some(ui) = weak.upgrade() else { return };
        crate::toast::outcome(&ui, caps::devices::set_grant(&key, &token, on));
        super::with(|c| c.poke());
    });
    let weak = ui.as_weak();
    h.on_set_files(move |key, level| {
        let Some(ui) = weak.upgrade() else { return };
        crate::toast::outcome(&ui, caps::devices::set_files(&key, level));
        super::with(|c| c.poke());
    });
    let weak = ui.as_weak();
    h.on_unpair(move |key| {
        let Some(ui) = weak.upgrade() else { return };
        crate::toast::outcome(&ui, caps::devices::unpair(&key));
        super::with(|c| c.poke());
    });
}

pub fn apply(ui: &AppWindow, ctl: &mut Ctl, s: &HostSnapshot) {
    let rows = s.devices.iter().map(|r| row(ui, r)).collect();
    sync(&ctl.models.devices, rows);
}

pub fn grants(g: &caps::devices::Grants) -> Grants {
    Grants {
        files: match g.files {
            "ro" => 1,
            "rw" => 2,
            _ => 0,
        },
        shells: g.shells,
        settings: g.settings,
        host: g.host,
        input: g.input,
        clipboard: g.clipboard,
    }
}

fn row(ui: &AppWindow, r: &Row) -> DeviceRow {
    let d = &r.device;
    let (words, tone, motion) = caps::devices::link_words(r.link.as_ref(), r.online);
    let name = if d.name.is_empty() {
        "Unnamed device"
    } else {
        &d.name
    };
    DeviceRow {
        key: d.key.as_str().into(),
        short_key: format!("{}…", d.key.chars().take(16).collect::<String>()).into(),
        name: name.into(),
        letters: initials(name).into(),
        hue: theme::hue(ui, theme::hue_index(&d.key)),
        owner: r.owner,
        pinned: d.pinned,
        online: r.online,
        link_words: words.into(),
        link_tone: tone.into(),
        link_motion: motion.into(),
        grants: grants(&r.grants),
        summary: wado::cli::devices::describe(&r.grants)
            .replace(',', " · ")
            .into(),
    }
}

/// `Pixel 8 · Chrome` → `P8`; `Linux · Firefox` → `LF`.
pub fn initials(name: &str) -> String {
    name.split(|c: char| c.is_whitespace() || c == '·' || c == '-')
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .collect::<String>()
        .to_uppercase()
}
