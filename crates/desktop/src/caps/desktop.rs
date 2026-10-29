// SPDX-License-Identifier: AGPL-3.0-only
//! This desktop itself: opening a URL in the person's browser. (The clipboard is the toolkit's —
//! the UI copies through a hidden text field, so it works on Wayland and X11 with no tool.)

use std::process::{Command, Stdio};

/// Open `url` with the desktop's handler. Only http(s): this is a link to the web client, never
/// an arbitrary command.
pub fn open_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only web addresses are opened".into());
    }
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("could not open the browser (xdg-open: {e})"))
}
