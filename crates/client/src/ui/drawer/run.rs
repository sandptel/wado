//! What the ▶ button and the Enter key actually launch.
//!
//! One rule, and it is here rather than inline because it is the difference between typing
//! `Files` and getting `Files: command not found`: text that names an installed application —
//! by its **name** or by its **command** — launches that application's `Exec`; anything else
//! is launched verbatim, flags and all.
//!
//! Case-insensitive on the match, never on the fallback: a hand-typed command is passed
//! through exactly as written, because the shell cares.

use wado_protocol::AppEntry;

/// The command to run for what is in the box. `None` when there is nothing to run.
pub fn resolve(typed: &str, apps: &[AppEntry]) -> Option<String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    let lower = typed.to_lowercase();
    Some(
        apps.iter()
            .find(|a| a.name.to_lowercase() == lower || a.exec.to_lowercase() == lower)
            .map(|a| a.exec.clone())
            .unwrap_or_else(|| typed.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, exec: &str) -> AppEntry {
        AppEntry {
            name: name.into(),
            exec: exec.into(),
            icon: None,
            running: false,
            hidden: false,
        }
    }

    #[test]
    fn a_name_resolves_to_the_command_it_names() {
        let apps = [app("Files", "nautilus"), app("Firefox", "firefox")];
        assert_eq!(resolve("Files", &apps).as_deref(), Some("nautilus"));
        // The case people actually type it in.
        assert_eq!(resolve("  files ", &apps).as_deref(), Some("nautilus"));
        // Already a command: itself, not a second lookup.
        assert_eq!(resolve("firefox", &apps).as_deref(), Some("firefox"));
    }

    #[test]
    fn anything_else_runs_as_written() {
        let apps = [app("Files", "nautilus")];
        // Flags survive, and so does the case — the shell is not case-insensitive.
        assert_eq!(resolve("htop -d 5", &apps).as_deref(), Some("htop -d 5"));
        assert_eq!(resolve("Xterm", &[]).as_deref(), Some("Xterm"));
        assert_eq!(resolve("   ", &apps), None);
        assert_eq!(resolve("", &apps), None);
    }
}
