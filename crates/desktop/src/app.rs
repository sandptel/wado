// SPDX-License-Identifier: AGPL-3.0-only
//! The window's own wiring: appearance (scheme, corners, motion), the clipboard, opening links,
//! and which mode it opens in.

use std::rc::Rc;

use slint::{ComponentHandle, VecModel};

use crate::{App, AppWindow, SchemeRow, Theme, caps, prefs, theme};

pub struct Options {
    /// `Some(0)` host · `Some(1)` client · `None` host if a daemon answers here.
    pub mode: Option<i32>,
    pub scheme: Option<String>,
    pub live: bool,
}

pub fn setup(ui: &AppWindow, o: &Options) {
    // The config the daemons use, so this computer's relay and client URL are the real ones.
    let _ = caps::config::load();
    let mut p = prefs::load();
    if let Some(s) = &o.scheme {
        p.scheme = s.clone();
    }
    let a = ui.global::<App>();
    a.set_version(env!("CARGO_PKG_VERSION").into());
    let schemes = theme::all();
    a.set_schemes(
        Rc::new(VecModel::from(
            schemes
                .iter()
                .map(|s| {
                    let g = s.slots[0];
                    SchemeRow {
                        name: s.name.as_str().into(),
                        ground: g,
                        text: s.slots[5],
                        accent: s.slots[13],
                        mauve: s.slots[14],
                        good: s.slots[11],
                    }
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );
    apply_prefs(ui, &p);

    let weak = ui.as_weak();
    a.on_pick_scheme(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let mut p = prefs::load();
        p.scheme = name.to_string();
        prefs::save(&p);
        apply_prefs(&ui, &p);
        crate::host::recolour(&ui);
    });
    let weak = ui.as_weak();
    a.on_pick_radius(move |r| {
        let Some(ui) = weak.upgrade() else { return };
        let mut p = prefs::load();
        p.radius = r;
        prefs::save(&p);
        apply_prefs(&ui, &p);
    });
    let weak = ui.as_weak();
    a.on_pick_motion(move |m| {
        let Some(ui) = weak.upgrade() else { return };
        let mut p = prefs::load();
        p.motion = m;
        prefs::save(&p);
        apply_prefs(&ui, &p);
    });
    let weak = ui.as_weak();
    a.on_copy(move |t| {
        let Some(ui) = weak.upgrade() else { return };
        ui.invoke_copy_text(t);
        crate::toast::say(&ui, "Copied", "good");
    });
    let weak = ui.as_weak();
    a.on_open_url(move |u| {
        let Some(ui) = weak.upgrade() else { return };
        if let Err(e) = caps::desktop::open_url(&u) {
            crate::toast::say(&ui, &e, "bad");
        }
    });

    // Host mode when a daemon answers here, or one has run here (it has a Remote ID); client
    // mode otherwise.
    let mode = o.mode.unwrap_or_else(|| {
        let here =
            !wado::cli::status::daemons().is_empty() || caps::computers::this_computer().is_some();
        if here { 0 } else { 1 }
    });
    a.set_mode(mode);
}

fn apply_prefs(ui: &AppWindow, p: &prefs::Prefs) {
    let s = theme::find(&p.scheme);
    theme::apply(ui, &s);
    ui.global::<App>().set_scheme(s.name.as_str().into());
    let t = ui.global::<Theme>();
    t.set_radius_mode(p.radius.clamp(0, 2));
    t.set_motion(p.motion.clamp(0, 2));
}
