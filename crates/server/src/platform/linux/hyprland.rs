//! Hyprland ([`WindowManager`] on Linux): which instance to talk to, and `hyprctl dispatch`.
//!
//! `hyprctl` finds Hyprland by `HYPRLAND_INSTANCE_SIGNATURE`, a directory under
//! `$XDG_RUNTIME_DIR/hypr`. The daemon's own is captured at start (like `clip::env`), but a
//! Hyprland restarted since has a new one, so the captured signature is used only while its
//! socket still exists — otherwise the newest instance's.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use tokio::process::Command;
use wado_platform::desktop::{Dir, WindowManager, WindowOp};
use wado_platform::{Fut, Res};

const DEADLINE: Duration = Duration::from_secs(5);

/// Shown when there is no Hyprland to drive.
pub const NOT_RUNNING: &str = "Hyprland is not running on this computer";

/// The window manager, through `hyprctl`.
pub struct Hyprland;

impl Hyprland {
    /// Hyprland, when `hyprctl` is installed and an instance answers it — the probe, run where
    /// blocking is fine (daemon start, or a blocking task).
    pub fn probe() -> Result<Hyprland, String> {
        let sig = signature().ok_or(NOT_RUNNING)?;
        let mut cmd = std::process::Command::new("hyprctl");
        cmd.arg("version").env("HYPRLAND_INSTANCE_SIGNATURE", sig);
        match super::cmd::run_blocking(cmd, DEADLINE) {
            Some(out) if out.status.success() => Ok(Hyprland),
            _ => Err(NOT_RUNNING.into()),
        }
    }
}

impl WindowManager for Hyprland {
    fn run(&self, op: WindowOp) -> Fut<'_, Res<()>> {
        Box::pin(async move { dispatch(&args(op)).await })
    }
}

/// The `hyprctl` arguments that do `op`.
pub fn args(op: WindowOp) -> Vec<String> {
    let d = |a: &[&str]| {
        ["dispatch"]
            .iter()
            .chain(a)
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    let side = |dir| if dir == Dir::Left { "l" } else { "r" };
    match op {
        WindowOp::Fullscreen => d(&["fullscreen"]),
        WindowOp::Close => d(&["killactive"]),
        WindowOp::ToggleFloat => d(&["togglefloating"]),
        WindowOp::Pin => d(&["pin"]),
        WindowOp::Focus(dir) => d(&["movefocus", side(dir)]),
        // `e±1`: the next workspace that has windows, so a swipe never lands on an empty one.
        WindowOp::WorkspaceStep(Dir::Left) => d(&["workspace", "e-1"]),
        WindowOp::WorkspaceStep(Dir::Right) => d(&["workspace", "e+1"]),
        WindowOp::Workspace(n) => d(&["workspace", &n.to_string()]),
        WindowOp::MoveToWorkspace(n) => d(&["movetoworkspace", &n.to_string()]),
    }
}

static CAPTURED: OnceLock<Option<String>> = OnceLock::new();

/// Call at daemon start. Later calls change nothing.
pub fn capture() {
    CAPTURED.get_or_init(|| {
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .ok()
            .filter(|s| !s.is_empty())
    });
}

/// `$XDG_RUNTIME_DIR/hypr`.
fn dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        // SAFETY: getuid cannot fail and touches no memory.
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })))
        .join("hypr")
}

/// The signature to use now: the captured one while its socket exists, else the newest.
pub fn signature() -> Option<String> {
    resolve(CAPTURED.get().cloned().flatten().as_deref(), &dir())
}

/// The captured signature while `dir/<it>/.socket.sock` exists, else the instance in `dir`
/// whose socket is newest.
pub fn resolve(captured: Option<&str>, dir: &Path) -> Option<String> {
    let socket = |sig: &str| dir.join(sig).join(".socket.sock");
    if let Some(sig) = captured.filter(|s| socket(s).exists()) {
        return Some(sig.to_string());
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let sig = e.file_name().into_string().ok()?;
            let at = socket(&sig).metadata().and_then(|m| m.modified()).ok()?;
            Some((at, sig))
        })
        .max()
        .map(|(_, sig)| sig)
}

/// `hyprctl args…` against the current instance; its stdout.
pub(super) async fn hyprctl(args: &[&str]) -> Result<String, String> {
    let sig = signature().ok_or("Hyprland is not running on this computer")?;
    let out = tokio::time::timeout(
        DEADLINE,
        Command::new("hyprctl")
            .args(args)
            .env("HYPRLAND_INSTANCE_SIGNATURE", sig)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "Hyprland took too long to answer".to_string())?
    .map_err(|e| format!("hyprctl is not available here ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(text)
    } else {
        Err(format!("Hyprland did not answer: {text}"))
    }
}

/// `hyprctl dispatch …`. It exits 0 even when it refuses, so its answer is read: `ok`.
pub async fn dispatch(args: &[String]) -> Result<(), String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let answer = hyprctl(&args).await?;
    if answer == "ok" {
        Ok(())
    } else {
        Err(format!("Hyprland refused: {answer}"))
    }
}

/// The focused workspace's id — for checking an action live.
pub async fn active_workspace() -> Result<i64, String> {
    let json = hyprctl(&["activeworkspace", "-j"]).await?;
    serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .and_then(|v| v["id"].as_i64())
        .ok_or_else(|| format!("unreadable answer: {json}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn instance(dir: &Path, sig: &str, age_s: u64) {
        std::fs::create_dir_all(dir.join(sig)).unwrap();
        let f = std::fs::File::create(dir.join(sig).join(".socket.sock")).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(age_s))
            .unwrap();
    }

    #[test]
    fn each_op_is_exactly_a_hyprctl_dispatch() {
        let a = |op| args(op).join(" ");
        assert_eq!(
            a(WindowOp::WorkspaceStep(Dir::Right)),
            "dispatch workspace e+1"
        );
        assert_eq!(
            a(WindowOp::WorkspaceStep(Dir::Left)),
            "dispatch workspace e-1"
        );
        assert_eq!(
            a(WindowOp::MoveToWorkspace(3)),
            "dispatch movetoworkspace 3"
        );
        assert_eq!(a(WindowOp::Workspace(9)), "dispatch workspace 9");
        assert_eq!(a(WindowOp::Focus(Dir::Left)), "dispatch movefocus l");
        assert_eq!(a(WindowOp::Focus(Dir::Right)), "dispatch movefocus r");
        assert_eq!(a(WindowOp::Fullscreen), "dispatch fullscreen");
        assert_eq!(a(WindowOp::Close), "dispatch killactive");
        assert_eq!(a(WindowOp::ToggleFloat), "dispatch togglefloating");
        assert_eq!(a(WindowOp::Pin), "dispatch pin");
    }

    #[test]
    fn prefers_the_captured_signature_else_the_newest() {
        let dir = std::env::temp_dir().join(format!("wado-hypr-{}", rand::random::<u64>()));
        instance(&dir, "old", 100);
        instance(&dir, "new", 1);
        instance(&dir, "mid", 50);
        assert_eq!(resolve(Some("old"), &dir).as_deref(), Some("old"));
        // Hyprland restarted: the captured one is gone.
        assert_eq!(resolve(Some("gone"), &dir).as_deref(), Some("new"));
        assert_eq!(resolve(None, &dir).as_deref(), Some("new"));
        // A directory without a socket is not an instance.
        std::fs::create_dir_all(dir.join("zzz")).unwrap();
        assert_eq!(resolve(None, &dir).as_deref(), Some("new"));
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(resolve(Some("old"), &dir), None, "no Hyprland at all");
    }
}
