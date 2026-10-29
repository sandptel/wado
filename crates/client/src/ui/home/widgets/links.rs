//! Which computers hold a light link (`js/side_link.js`), and when: every computer home has a
//! widget for, except the selected one — whose own link already carries everything — and none
//! at all while a desktop session streams, when the downlink belongs to the picture.

use dioxus::prelude::*;

use crate::{
    state::Ui,
    widget::{self, WidgetKind},
};

pub fn watch(ui: Ui) {
    let s = ui.set;
    let mut live = ui.live;
    use_effect(move || {
        let streaming = (live.session_on)();
        let list = s.widgets.read().clone();
        let profiles = s.profiles.read().clone();
        let cur = profiles.get((s.profile)()).map(|p| p.key());
        let targets: Vec<(String, String, String, Vec<&str>)> = if streaming {
            Vec::new()
        } else {
            widget::hosts(&list)
                .into_iter()
                .filter(|k| Some(k) != cur.as_ref())
                .filter_map(|k| {
                    let p = profiles.iter().find(|p| p.key() == k)?;
                    if p.conn_mode != "relay" || p.remote_id.is_empty() {
                        return None;
                    }
                    let mut wants = Vec::new();
                    if list
                        .iter()
                        .any(|w| w.host == k && w.kind == WidgetKind::Clipboard)
                    {
                        wants.push("clips");
                    }
                    if list
                        .iter()
                        .any(|w| w.host == k && w.kind == WidgetKind::Trackpad)
                    {
                        wants.push("actions");
                    }
                    Some((k, p.relay_url.clone(), p.remote_id.clone(), wants))
                })
                .collect()
        };
        // What a dropped link last said is not news any more.
        let keep: Vec<String> = targets.iter().map(|t| t.0.clone()).collect();
        if live.side.peek().keys().any(|k| !keep.contains(k)) {
            live.side.write().retain(|k, _| keep.contains(k));
        }
        crate::bridge::call(format!(
            "window.__wado.sideWatch({});",
            crate::bridge::js(&targets)
        ));
    });
}
