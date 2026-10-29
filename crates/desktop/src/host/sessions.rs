// SPDX-License-Identifier: AGPL-3.0-only
//! Sessions: each running desktop of the pool, its shape, apps, and who watches it how.

use super::Ctl;
use crate::{
    AppWindow, SessionRow,
    caps::{self, watch::HostSnapshot},
    models::{strings, sync},
};

pub fn apply(_ui: &AppWindow, ctl: &mut Ctl, s: &HostSnapshot) {
    if !ctl.changed("sessions", format!("{:?}{:?}", s.sessions, s.session_links)) {
        return;
    }
    let rows = s
        .sessions
        .iter()
        .zip(s.session_links.iter())
        .map(|(x, link)| {
            let (words, tone, motion) =
                caps::devices::link_words(link.as_ref(), x.viewer.is_some());
            SessionRow {
                instance: x.instance.as_str().into(),
                daemon: caps::sessions::daemon_of(x).into(),
                shape: format!("{}×{}", x.width, x.height).into(),
                fps: x.fps as i32,
                age: caps::sessions::age(x.age_s).into(),
                apps: strings(x.apps.iter().cloned()),
                viewer: x.viewer.clone().unwrap_or_default().into(),
                link_words: if x.viewer.is_some() {
                    words
                } else {
                    "—".into()
                }
                .into(),
                link_tone: tone.into(),
                link_motion: motion.into(),
                detached: x.detached,
            }
        })
        .collect();
    sync(&ctl.models.sessions, rows);
}
