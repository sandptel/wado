//! Tap targets: the actionable elements under a fingertip, as the phone shell's precision
//! features see them (magnetic taps, the lens). Read from the application's accessibility tree
//! by the server — see `wado::a11y`.

use serde::{Deserialize, Serialize};

/// One actionable element near a touch, in the output's normalized 0..1 coordinates (the same
/// space the client's touches are sent in).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// AT-SPI role name — "button", "toggle button", "menu item", …
    pub role: String,
    /// Accessible name; empty when the app gives none.
    pub name: String,
}
