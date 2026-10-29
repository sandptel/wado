// SPDX-License-Identifier: AGPL-3.0-only
//! The host console: this computer's daemons, devices, pairing, sessions, logs and config.
//!
//! State arrives as `caps::watch::HostSnapshot`s (pushed, see `crate::bridge`); [`apply`] turns
//! each into the `Host` global's models, one page module per job. Actions run on the UI thread
//! — they are small file writes through `caps` — then poke the producer so the next snapshot
//! shows their effect at once.
//!
//! - [`overview`] — the hero, live stats, activity.
//! - [`devices`] — the trust list and grants.
//! - [`pairing`] — the code, QR, countdown and check words.
//! - [`sessions`] — running desktops.
//! - [`logs`] — the daemon log.
//! - [`settings`] — config.kdl.

mod devices;
mod logs;
mod overview;
mod pairing;
mod sessions;
mod settings;

pub use devices::initials;
pub use pairing::recolour;

use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};

use slint::{ComponentHandle, VecModel};

use crate::{
    AppWindow, Host,
    caps::watch::{self, HostSnapshot, Producer, ShownCode},
};

/// The console's UI-thread state.
pub struct Ctl {
    producer: Producer<Option<Arc<HostSnapshot>>>,
    last: Option<Arc<HostSnapshot>>,
    pub pairing: pairing::State,
    pub settings: settings::State,
    pub logs: logs::State,
    pub models: Models,
    /// What each model was last built from, so an unchanged source is not rebuilt (rows that
    /// hold a list compare by identity, so equal data would otherwise look new every second).
    sigs: std::collections::HashMap<&'static str, String>,
}

pub struct Models {
    pub devices: Rc<VecModel<crate::DeviceRow>>,
    pub sessions: Rc<VecModel<crate::SessionRow>>,
    pub logs: Rc<VecModel<crate::LogLine>>,
    pub activity: Rc<VecModel<crate::LogLine>>,
    pub asks: Rc<VecModel<crate::AskRow>>,
    pub meta: Rc<VecModel<crate::Stat>>,
    pub live: Rc<VecModel<crate::Stat>>,
    pub sections: Rc<VecModel<crate::ConfigSection>>,
    pub fields: Rc<VecModel<crate::ConfigField>>,
}

thread_local! {
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

/// Run `f` on the console's state, if the console is installed.
pub fn with<R>(f: impl FnOnce(&mut Ctl) -> R) -> Option<R> {
    CTL.with(|c| c.borrow_mut().as_mut().map(f))
}

impl Ctl {
    /// Look again now.
    pub fn poke(&self) {
        self.producer.poke();
    }
    pub fn last(&self) -> Option<&Arc<HostSnapshot>> {
        self.last.as_ref()
    }
    /// True when `sig` differs from what `name` was last built from (and remembers it).
    pub fn changed(&mut self, name: &'static str, sig: String) -> bool {
        if self.sigs.get(name) == Some(&sig) {
            return false;
        }
        self.sigs.insert(name, sig);
        true
    }
}

/// Install the console: models into the `Host` global, callbacks wired, the producer started
/// and subscribed. `live: false` (screenshots) applies one snapshot read here instead.
pub fn install(ui: &AppWindow, live: bool) {
    let models = Models {
        devices: Rc::new(VecModel::default()),
        sessions: Rc::new(VecModel::default()),
        logs: Rc::new(VecModel::default()),
        activity: Rc::new(VecModel::default()),
        asks: Rc::new(VecModel::default()),
        meta: Rc::new(VecModel::default()),
        live: Rc::new(VecModel::default()),
        sections: Rc::new(VecModel::default()),
        fields: Rc::new(VecModel::default()),
    };
    let h = ui.global::<Host>();
    use crate::models::rc;
    h.set_devices(rc(&models.devices));
    h.set_sessions(rc(&models.sessions));
    h.set_logs(rc(&models.logs));
    h.set_activity(rc(&models.activity));
    h.set_asks(rc(&models.asks));
    h.set_meta(rc(&models.meta));
    h.set_live(rc(&models.live));
    h.set_sections(rc(&models.sections));
    h.set_fields(rc(&models.fields));

    let shown: ShownCode = Default::default();
    let producer = watch::host(shown.clone(), Duration::from_secs(1));
    let rx = producer.rx.clone();
    CTL.with(|c| {
        *c.borrow_mut() = Some(Ctl {
            producer,
            last: None,
            pairing: pairing::State::new(shown),
            settings: settings::State::default(),
            logs: logs::State::default(),
            models,
            sigs: Default::default(),
        })
    });

    overview::wire(ui);
    devices::wire(ui);
    pairing::wire(ui);
    logs::wire(ui);
    settings::wire(ui);
    settings::sections(ui);

    if live {
        crate::bridge::subscribe(rx, ui.as_weak(), apply);
    }
}

/// Apply a snapshot read synchronously — for the screenshot mode, which has no event loop
/// running to receive pushes.
#[allow(dead_code)]
pub fn apply_now(ui: &AppWindow, snap: HostSnapshot) {
    apply(ui, Some(Arc::new(snap)));
}

/// A new snapshot: every page takes what it shows from it.
fn apply(ui: &AppWindow, snap: Option<Arc<HostSnapshot>>) {
    let Some(snap) = snap else { return };
    with(|ctl| {
        overview::apply(ui, ctl, &snap);
        devices::apply(ui, ctl, &snap);
        pairing::apply(ui, ctl, &snap);
        sessions::apply(ui, ctl, &snap);
        logs::apply(ui, ctl, &snap);
        settings::apply(ui, ctl, &snap);
        ctl.last = Some(snap);
    });
}

/// Read and apply a snapshot now, on this thread — for the screenshot mode, which has no
/// subscription running.
pub fn refresh_now(ui: &AppWindow) {
    let Some(shown) = with(|c| c.pairing.shown()) else {
        return;
    };
    let mut tail = crate::caps::logs::Tail::default();
    let snap = watch::read_host(&crate::caps::daemon::hostname(), &shown, &mut tail);
    apply(ui, Some(Arc::new(snap)));
}

/// Withdraw the code on screen, if one is live — on the way out of a screenshot run.
pub fn withdraw_shown() {
    with(|c| {
        if let Some(m) = c.pairing.shown().lock().ok().and_then(|m| m.clone()) {
            crate::caps::pairing::withdraw(&m);
        }
    });
}
