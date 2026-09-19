//! The recently-launched row: bare commands turned back into something with a name and an icon.
//!
//! Recents are stored as the `Exec` string alone (see [`crate::state::Settings::recent`]), so
//! this is where they are re-joined with the live application list. A command that matches no
//! installed entry still comes back — as itself, which is honest and still launchable.

use wado_protocol::AppEntry;

/// Resolve stored commands against the current app list, in order.
pub fn entries(commands: &[String], apps: &[AppEntry]) -> Vec<AppEntry> {
    commands
        .iter()
        .map(|cmd| {
            apps.iter()
                .find(|a| &a.exec == cmd)
                .cloned()
                .unwrap_or_else(|| AppEntry {
                    name: cmd.clone(),
                    exec: cmd.clone(),
                    icon: None,
                    // Unknown rather than false, strictly — but the list it would have been
                    // matched against is the one that did not contain it.
                    running: false,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_with_no_matching_entry_survives_as_itself() {
        let apps = vec![AppEntry {
            name: "Files".into(),
            exec: "nautilus".into(),
            icon: Some("data:image/png;base64,AAAA".into()),
            running: true,
        }];
        let got = entries(&["nautilus".into(), "htop -d 5".into()], &apps);
        // Order is the caller's: most recent first, and resolution must not reorder it.
        assert_eq!(got[0].name, "Files");
        assert!(got[0].icon.is_some());
        assert!(got[0].running, "the live entry's state must come through");
        assert_eq!(got[1].name, "htop -d 5");
        assert_eq!(got[1].icon, None);
    }
}
