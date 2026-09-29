//! Native menu sheets (M-P S7): an app's open menu, read from its accessibility tree and shown
//! on the phone as a bottom sheet of finger-sized rows. Picking a row activates the item through
//! the tree — no aiming at 20-pixel rows.

use serde::{Deserialize, Serialize};

/// The menu open on the focused window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuSheet {
    /// Its items, top to bottom. Empty with `tree: false`.
    pub items: Vec<MenuItem>,
    /// Whether the items came from the app's tree. `false`: it exposes none, and the client
    /// falls back to the lens over the real popup at `x, y, w, h`.
    pub tree: bool,
    /// The popup, in normalized output coordinates.
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One row of a menu sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuItem {
    /// Opaque handle, sent back to activate it (an object path on the app's bus).
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// A check or radio item that is currently on.
    pub checked: bool,
    /// Opens a further menu rather than acting.
    pub submenu: bool,
}
