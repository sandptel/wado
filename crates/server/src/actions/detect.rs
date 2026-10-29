//! What this computer has for the actions to use: a running Hyprland, the tools on `PATH`, a
//! session bus for media players, the desktop's display, a terminal.
//!
//! Looked up at most every 30 s, and again after a miss — an action that failed or was asked for
//! while missing — so a Hyprland restarted or a tool just installed shows up on the next list.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FRESH: Duration = Duration::from_secs(30);

/// One thing an action needs here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    /// Hyprland is running and `hyprctl` reaches it.
    Hyprland,
    /// A program on `PATH`.
    Tool(&'static str),
    /// A session bus, where media players (MPRIS) live.
    Mpris,
    /// The desktop's Wayland display (`clip::env`), for tools that draw or capture on it.
    Desktop,
    /// `xdg-terminal-exec`, else `$TERMINAL`.
    Terminal,
}

impl Need {
    /// Why an action needing this cannot run here, in plain words.
    pub fn why(&self) -> String {
        match self {
            Need::Hyprland => "Hyprland is not running on this computer".into(),
            Need::Tool(t) => format!("{t} is not installed on this computer"),
            Need::Mpris => "this computer has no desktop session bus for media players".into(),
            Need::Desktop => "the daemon was not started inside a Wayland desktop".into(),
            Need::Terminal => {
                "no terminal found — install xdg-terminal-exec or set $TERMINAL".into()
            }
        }
    }
}

/// Answers "is this here?" — the real computer, or a fake in tests.
pub trait Detect {
    fn has(&self, need: &Need) -> bool;
}

/// What was found, once.
#[derive(Debug, Clone, Default)]
pub struct Found {
    hyprland: bool,
    mpris: bool,
    desktop: bool,
    tools: BTreeSet<&'static str>,
    /// The terminal program to start, when there is one.
    pub terminal: Option<String>,
}

impl Detect for Found {
    fn has(&self, need: &Need) -> bool {
        match need {
            Need::Hyprland => self.hyprland,
            Need::Tool(t) => self.tools.contains(t),
            Need::Mpris => self.mpris,
            Need::Desktop => self.desktop,
            Need::Terminal => self.terminal.is_some(),
        }
    }
}

static CACHE: Mutex<Option<(Instant, Found)>> = Mutex::new(None);

/// What this computer has, from the cache while it is fresh.
pub async fn now() -> Found {
    if let Some((at, found)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && at.elapsed() < FRESH
    {
        return found.clone();
    }
    let found = scan().await;
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), found.clone()));
    found
}

/// Look again on the next [`now`].
pub fn forget() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

async fn scan() -> Found {
    let tools: BTreeSet<&'static str> = super::all()
        .iter()
        .flat_map(|a| a.needs.iter())
        .filter_map(|n| match n {
            Need::Tool(t) => Some(*t),
            _ => None,
        })
        .chain(["hyprctl"])
        .filter(|t| on_path(t))
        .collect();
    let hyprland = tools.contains("hyprctl") && super::hypr::running().await;
    Found {
        hyprland,
        mpris: session_bus(),
        desktop: crate::clip::env::display().is_some(),
        terminal: terminal(&|t| on_path(t), std::env::var("TERMINAL").ok()),
        tools,
    }
}

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

/// `xdg-terminal-exec` when installed, else `$TERMINAL` when it names a program that is.
fn terminal(exists: &dyn Fn(&str) -> bool, env: Option<String>) -> Option<String> {
    if exists("xdg-terminal-exec") {
        return Some("xdg-terminal-exec".into());
    }
    env.filter(|t| !t.trim().is_empty() && exists(t.trim()))
        .map(|t| t.trim().to_string())
}

fn session_bus() -> bool {
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some_and(|a| !a.is_empty())
        || std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|d| Path::new(&d).join("bus").exists())
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
    }
}
