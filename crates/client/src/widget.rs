//! Home widgets, as data: what kinds there are, what each needs, and the list's edits.
//!
//! A widget is one control bound to one computer — `host` is that computer's
//! [`crate::profile::Profile::key`], so it follows the computer through a rename. The list is
//! this phone's own setting (decided 2026-10-30: widgets live on the phone, not on each
//! computer), so it persists with the rest of [`crate::persist::Saved`] and rides the device's
//! prefs blob to the daemon like every other setting. Order is the list's order.
//!
//! No rendering here: `ui/home/widgets/` draws them.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Media,
    Volume,
    Brightness,
    Clipboard,
    Trackpad,
}

/// The grant a kind needs on its computer (`wado allow <device> <grant>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Host,
    Clipboard,
    Input,
}

impl Need {
    pub fn token(self) -> &'static str {
        match self {
            Need::Host => "host",
            Need::Clipboard => "clipboard",
            Need::Input => "input",
        }
    }
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 5] = [
        WidgetKind::Media,
        WidgetKind::Volume,
        WidgetKind::Brightness,
        WidgetKind::Clipboard,
        WidgetKind::Trackpad,
    ];

    /// The header's label.
    pub fn label(self) -> &'static str {
        match self {
            WidgetKind::Media => "Media player",
            WidgetKind::Volume => "Volume",
            WidgetKind::Brightness => "Brightness",
            WidgetKind::Clipboard => "Clipboard",
            WidgetKind::Trackpad => "Trackpad",
        }
    }

    /// The add tray's label, and the word in "may not control X's …".
    pub fn short(self) -> &'static str {
        match self {
            WidgetKind::Media => "Media",
            other => other.label(),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            WidgetKind::Media => "music",
            WidgetKind::Volume => "sound",
            WidgetKind::Brightness => "sun",
            WidgetKind::Clipboard => "clip",
            WidgetKind::Trackpad => "trackpad",
        }
    }

    pub fn needs(self) -> Need {
        match self {
            WidgetKind::Media | WidgetKind::Volume | WidgetKind::Brightness => Need::Host,
            WidgetKind::Clipboard => Need::Clipboard,
            WidgetKind::Trackpad => Need::Input,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    pub kind: WidgetKind,
    /// The computer's profile key (`relay|remote id`).
    pub host: String,
}

/// Add a widget of `kind` for `host` at the end; returns its id. A kind may be added any number
/// of times — for different computers, or twice for one.
pub fn add(list: &mut Vec<Widget>, kind: WidgetKind, host: &str) -> String {
    let n = list
        .iter()
        .filter_map(|w| w.id.strip_prefix('w').and_then(|n| n.parse::<u32>().ok()))
        .max()
        .map_or(1, |m| m + 1);
    let id = format!("w{n}");
    list.push(Widget {
        id: id.clone(),
        kind,
        host: host.to_string(),
    });
    id
}

pub fn remove(list: &mut Vec<Widget>, id: &str) {
    list.retain(|w| w.id != id);
}

/// One step up (`-1`) or down (`1`). Nothing at either end.
pub fn step(list: &mut [Widget], id: &str, by: i32) {
    let Some(i) = list.iter().position(|w| w.id == id) else {
        return;
    };
    let j = i as i64 + by as i64;
    if j < 0 || j >= list.len() as i64 {
        return;
    }
    list.swap(i, j as usize);
}

/// Move the widget at `from` to `to` — a drag's drop.
pub fn place(list: &mut Vec<Widget>, from: usize, to: usize) {
    if from >= list.len() || to >= list.len() || from == to {
        return;
    }
    let w = list.remove(from);
    list.insert(to, w);
}

pub fn rebind(list: &mut [Widget], id: &str, host: &str) {
    if let Some(w) = list.iter_mut().find(|w| w.id == id) {
        w.host = host.to_string();
    }
}

/// A computer was forgotten: its widgets go with it.
pub fn forget_host(list: &mut Vec<Widget>, host: &str) {
    list.retain(|w| w.host != host);
}

/// A computer's key changed (its Remote ID or relay was edited): its widgets follow it.
pub fn rekey(list: &mut [Widget], from: &str, to: &str) {
    if from == to {
        return;
    }
    for w in list.iter_mut().filter(|w| w.host == from) {
        w.host = to.to_string();
    }
}

/// The computers that have at least one widget, in first-seen order.
pub fn hosts(list: &[Widget]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in list {
        if !out.contains(&w.host) {
            out.push(w.host.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(l: &[Widget]) -> Vec<&str> {
        l.iter().map(|w| w.id.as_str()).collect()
    }

    #[test]
    fn ids_are_fresh_even_after_a_removal() {
        let mut l = Vec::new();
        add(&mut l, WidgetKind::Media, "a");
        add(&mut l, WidgetKind::Volume, "a");
        remove(&mut l, "w1");
        let id = add(&mut l, WidgetKind::Media, "b");
        assert_eq!(id, "w3");
        assert_eq!(ids(&l), ["w2", "w3"]);
    }

    #[test]
    fn steps_and_drops_reorder_and_stop_at_the_ends() {
        let mut l = Vec::new();
        for k in [WidgetKind::Media, WidgetKind::Volume, WidgetKind::Clipboard] {
            add(&mut l, k, "a");
        }
        step(&mut l, "w1", -1);
        assert_eq!(ids(&l), ["w1", "w2", "w3"]);
        step(&mut l, "w1", 1);
        assert_eq!(ids(&l), ["w2", "w1", "w3"]);
        place(&mut l, 2, 0);
        assert_eq!(ids(&l), ["w3", "w2", "w1"]);
        place(&mut l, 0, 9);
        assert_eq!(ids(&l), ["w3", "w2", "w1"]);
    }

    #[test]
    fn a_forgotten_computer_takes_its_widgets_and_a_rekey_keeps_them() {
        let mut l = Vec::new();
        add(&mut l, WidgetKind::Media, "a");
        add(&mut l, WidgetKind::Volume, "b");
        add(&mut l, WidgetKind::Trackpad, "a");
        assert_eq!(hosts(&l), ["a", "b"]);
        rekey(&mut l, "a", "c");
        assert_eq!(hosts(&l), ["c", "b"]);
        forget_host(&mut l, "c");
        assert_eq!(ids(&l), ["w2"]);
    }

    #[test]
    fn the_saved_shape_is_plain_json() {
        let w = Widget {
            id: "w1".into(),
            kind: WidgetKind::Brightness,
            host: "r|1".into(),
        };
        let j = serde_json::to_string(&w).unwrap();
        assert_eq!(j, r#"{"id":"w1","kind":"brightness","host":"r|1"}"#);
        assert_eq!(WidgetKind::Clipboard.needs().token(), "clipboard");
    }
}
