//! What this computer has, looked up cheaply for the providers' probes: a program on `PATH`, a
//! session bus, a terminal.

use std::path::Path;

/// `prog` is an executable on `PATH`, or an executable path itself.
pub fn on_path(prog: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let exec = |p: &Path| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if prog.contains('/') {
        return exec(Path::new(prog));
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|d| exec(&d.join(prog))))
}

/// `prog` is installed — else the words saying it is not.
pub fn tool(prog: &str) -> Result<(), String> {
    if on_path(prog) {
        Ok(())
    } else {
        Err(format!("{prog} is not installed on this computer"))
    }
}

/// A session bus, where media players (MPRIS) live.
pub fn session_bus() -> bool {
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some_and(|a| !a.is_empty())
        || std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|d| Path::new(&d).join("bus").exists())
}

/// Shown when there is no session bus.
pub const NO_BUS: &str = "this computer has no desktop session bus for media players";

/// Shown when there is no terminal to open.
pub const NO_TERMINAL: &str = "no terminal found — install xdg-terminal-exec or set $TERMINAL";

/// The terminal to open: `xdg-terminal-exec` when installed, else `$TERMINAL`.
pub fn terminal_here() -> Option<String> {
    terminal(&|t| on_path(t), std::env::var("TERMINAL").ok())
}

/// `xdg-terminal-exec` when installed, else `$TERMINAL` when it names a program that is.
fn terminal(exists: &dyn Fn(&str) -> bool, env: Option<String>) -> Option<String> {
    if exists("xdg-terminal-exec") {
        return Some("xdg-terminal-exec".into());
    }
    env.filter(|t| !t.trim().is_empty() && exists(t.trim()))
        .map(|t| t.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_prefers_xdg_terminal_exec_then_the_env() {
        let all = |_: &str| true;
        assert_eq!(
            terminal(&all, Some("kitty".into())).as_deref(),
            Some("xdg-terminal-exec")
        );
        let kitty = |t: &str| t == "kitty";
        assert_eq!(
            terminal(&kitty, Some("kitty".into())).as_deref(),
            Some("kitty")
        );
        assert_eq!(terminal(&kitty, Some("foot".into())), None, "not installed");
        assert_eq!(terminal(&kitty, None), None);
    }

    #[test]
    fn on_path_finds_a_shell() {
        assert!(on_path("sh"));
        assert!(!on_path("wado-no-such-tool"));
        assert_eq!(
            tool("wado-no-such-tool"),
            Err("wado-no-such-tool is not installed on this computer".into())
        );
    }
}
