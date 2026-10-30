//! One computer's clipboard history as this page knows it — from the selected computer's own
//! link, or from the light link a widget keeps to another (`js/side_link.js`).

use std::collections::HashMap;

use dioxus::prelude::*;
use wado_protocol::ClipEntry;

use crate::state::{clip_img_key, Ui};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hist {
    /// The computer has answered.
    pub loaded: bool,
    /// It keeps a history; when not, `error` says why.
    pub available: bool,
    /// Pinned first, then newest first.
    pub entries: Vec<ClipEntry>,
    pub error: String,
    /// Image entries' pictures, by entry id.
    pub imgs: HashMap<String, String>,
}

/// Computer `pk`'s history, whichever link carries it.
pub fn of(ui: Ui, pk: &str, selected: bool) -> Hist {
    let (loaded, available, mut entries, error) = if selected {
        let h = (ui.live.clip_hist)();
        (h.loaded, h.available, h.entries, h.error)
    } else {
        match ui.live.side.read().get(pk) {
            Some(s) if s.state == "up" => (true, true, s.clips.clone(), String::new()),
            _ => (false, false, Vec::new(), String::new()),
        }
    };
    // Pinned first, keeping each group's order (newest first).
    entries.sort_by_key(|e| !e.pinned);
    let imgs = ui.live.clip_imgs.read();
    let imgs = entries
        .iter()
        .filter_map(|e| {
            imgs.get(&clip_img_key(pk, &e.id))
                .map(|d| (e.id.clone(), d.clone()))
        })
        .collect();
    Hist {
        loaded,
        available,
        entries,
        error,
        imgs,
    }
}
