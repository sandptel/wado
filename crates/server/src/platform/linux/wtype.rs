//! Committed text ([`TextInjector`] on Linux), typed into the desktop via `wtype` — the keyboard
//! device handles named keys and chords, but arbitrary Unicode text goes through the desktop
//! compositor's own IME path instead of a per-character US-table lookup.

use std::time::Duration;

use wado_platform::input::TextInjector;
use wado_platform::{Fut, Res};

/// Shown when `wtype` is not installed.
pub const NO_WTYPE: &str =
    "Typing text needs wtype on the computer — install it (keys and shortcuts still work).";

/// Typing through `wtype`. Offered when `wtype` is installed and the daemon has the desktop's
/// display to aim it at.
pub struct Wtype;

impl TextInjector for Wtype {
    fn type_text(&self, text: String) -> Fut<'_, Res<()>> {
        Box::pin(async move { type_text(&text).await })
    }
}

const DEADLINE: Duration = Duration::from_secs(5);

/// `wtype`'s arguments for typing `text` verbatim: everything after `--` is one literal
/// argument, never interpreted as flags.
pub fn args(text: &str) -> Vec<&str> {
    vec!["--", text]
}

/// A `wtype` command aimed at the desktop's display (never the compositor's own, which
/// overwrites `WAYLAND_DISPLAY` for the session it runs).
pub fn command() -> Result<tokio::process::Command, String> {
    super::on_desktop::command("wtype")
}

/// Type `text` on the desktop. Keys and shortcuts (the keyboard device) do not depend on this —
/// only committed text does.
pub async fn type_text(text: &str) -> Result<(), String> {
    let mut c = command()?;
    let mut child = c
        .args(args(text))
        .spawn()
        .map_err(|_| NO_WTYPE.to_string())?;
    let status = tokio::time::timeout(DEADLINE, child.wait())
        .await
        .map_err(|_| "wtype took too long".to_string())?
        .map_err(|e| format!("wtype: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("wtype failed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_one_argument_after_double_dash() {
        assert_eq!(args("-rf / ; ls"), vec!["--", "-rf / ; ls"]);
    }

    #[test]
    fn text_targets_the_desktop_display() {
        // Review Focus 5: the compositor overwrites WAYLAND_DISPLAY; text must use the
        // captured desktop one, never whatever is live when this runs.
        crate::clip::env::capture();
        match command() {
            Ok(c) => assert_eq!(
                c.as_std()
                    .get_envs()
                    .find(|(k, _)| *k == "WAYLAND_DISPLAY")
                    .and_then(|(_, v)| v)
                    .map(|v| v.to_os_string()),
                crate::clip::env::display().map(Into::into)
            ),
            Err(_) => assert!(crate::clip::env::display().is_none()),
        }
    }
}
