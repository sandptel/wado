//! The debug registry: every debug view declared exactly once.
//!
//! Before this, adding a debug view meant edits in four places — a signal, a checkbox, a
//! `document::eval` call, and a render gate — and it was easy to wire up three of them. The
//! `latency` toggle was wired to the display and not to the work, so switching it off left a
//! 2 Hz ping running on the input data channel. A table makes that mistake impossible to make
//! quietly: one row is the whole declaration, and [`apply`] is the only thing that pushes it.
//!
//! Adding a view is a row here plus, if it drives browser-side work, a `window.__wado` setter.

use dioxus::prelude::*;

/// One debug view.
pub struct Item {
    /// Stable key. Persisted, so renaming it resets that toggle for existing users.
    pub id: &'static str,
    pub label: &'static str,
    /// Value on a fresh install. Kept as-is from before the registry existed, so nobody's
    /// visible readout changed when the master switch was introduced.
    pub default: bool,
    /// `window.__wado.<name>(bool)` to call when the effective value changes. `None` for a
    /// pure view gate whose state Rust already owns and acts on directly.
    pub js: Option<&'static str>,
}

pub const ITEMS: &[Item] = &[
    Item {
        // Off by default, and that is the fix rather than the preference: it used to be a
        // permanent full-width bar pinned along the top of the stream, covering the row
        // where an application's own controls live — you could not tell where to tap.
        id: "statusbar",
        label: "Status overlay (session, encoder, stats)",
        default: false,
        js: None,
    },
    Item {
        // On by default, unlike everything else here. It is the one view that answers the
        // question people actually have when the picture misbehaves — *whose fault is it* —
        // and it draws nothing at all while the answer is "nobody's".
        id: "health",
        label: "Health verdict (who is at fault)",
        default: true,
        js: None,
    },
    Item {
        id: "fps",
        label: "Show FPS",
        default: true,
        js: None,
    },
    Item {
        id: "ping",
        label: "Show ping",
        default: true,
        js: None,
    },
    Item {
        id: "touches",
        label: "Show touches / clicks",
        default: false,
        js: Some("setShowTouches"),
    },
    Item {
        id: "latency",
        label: "Latency breakdown (per stage)",
        default: false,
        js: Some("setLatency"),
    },
];

/// Position of `id` in [`ITEMS`]. Panics on an unknown id, which can only be a typo in code.
pub fn index_of(id: &str) -> usize {
    ITEMS
        .iter()
        .position(|i| i.id == id)
        .unwrap_or_else(|| panic!("unknown debug item {id:?}"))
}

/// Whether `id` is actually in effect: its own flag AND the master switch.
///
/// Everything that renders or runs on behalf of a debug view must go through this, so
/// flipping the master is enough to silence the group.
pub fn on(ui: crate::state::Ui, id: &str) -> bool {
    (ui.set.debug_master)()
        && ui
            .set
            .debug
            .read()
            .get(index_of(id))
            .copied()
            .unwrap_or(false)
}

/// Push every item's effective value to its JS setter.
///
/// Called after any change to the master or to one item — pushing all of them rather than
/// just the one that moved keeps the master switch a single code path instead of a fan-out.
pub fn apply(ui: crate::state::Ui) {
    let master = (ui.set.debug_master)();
    let flags = ui.set.debug.read().clone();
    for (i, item) in ITEMS.iter().enumerate() {
        let Some(setter) = item.js else { continue };
        let value = master && flags.get(i).copied().unwrap_or(false);
        spawn(async move {
            let _ = document::eval(&format!("window.__wado.{setter}({value});")).await;
        });
    }
}
