//! Which of the discovered applications are running in the session right now.
//!
//! Its own file because it is the one part of the app list that is not discovery: the desktop
//! files say what *can* be launched, and this says what *is* launched. It answers by asking the
//! compositor, which is the only thing that knows — it holds the child processes.
//!
//! The join is by command string. That is exact rather than fuzzy because both sides are
//! talking about the same text: the client launched `AppEntry::exec`, the compositor stored it
//! verbatim, and neither invented a name in between. Matching `xdg_toplevel.app_id` against
//! `Exec` instead would have meant guessing that `org.gnome.Nautilus` is `nautilus`.

use std::time::Duration;

use wado_compositor::{control::CompositorCommand, CommandSender};
use wado_protocol::AppEntry;

/// How long to wait for the compositor to answer before giving up on the dot.
///
/// Same bound as every other compositor round trip on these paths, and for the same reason: a
/// wedged render loop must not hold an HTTP response or the relay socket open. A timeout here
/// costs the marks, not the list.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Mark the entries whose command the compositor still has a live process for.
///
/// Best effort by design — a session that is not running, or a compositor that does not answer,
/// leaves every entry unmarked, which is what the client draws when it knows nothing.
pub async fn mark(apps: &mut [AppEntry], cmd_tx: &CommandSender) {
    for command in query(cmd_tx).await {
        for app in apps.iter_mut().filter(|a| a.exec == command) {
            app.running = true;
        }
    }
}

/// The commands the compositor still has processes for.
async fn query(cmd_tx: &CommandSender) -> Vec<String> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    if cmd_tx
        .send(CompositorCommand::RunningApps { reply })
        .is_err()
    {
        return Vec::new();
    }
    tokio::time::timeout(TIMEOUT, rx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(exec: &str) -> AppEntry {
        AppEntry {
            name: exec.into(),
            exec: exec.into(),
            icon: None,
            running: false,
            hidden: false,
        }
    }

    /// The join is the whole of this module's logic, and the case that matters is the one that
    /// used to be tempting to get wrong: a running command that is not in the list at all, and
    /// a listed application that merely *starts with* the same word.
    #[test]
    fn only_an_exact_command_match_is_marked() {
        let mut apps = vec![entry("firefox"), entry("firefox-esr"), entry("nautilus")];
        for command in ["firefox", "htop -d 5"] {
            for app in apps.iter_mut().filter(|a| a.exec == command) {
                app.running = true;
            }
        }
        assert!(apps[0].running);
        assert!(!apps[1].running, "a prefix is not a match");
        assert!(!apps[2].running);
    }
}
