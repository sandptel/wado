//! Running a tool on the desktop's display (the one captured before the compositor replaced
//! `WAYLAND_DISPLAY`, `crate::clip::env`) — awaited with a deadline, fed input, or started and
//! left to run. Arguments are arguments, never a shell string. (Tools that need no display go
//! through `platform::linux::cmd`.)

use std::process::Stdio;
use std::time::Duration;

use tokio::{io::AsyncWriteExt, process::Command};

/// How long a clipboard tool may take.
const CLIP_DEADLINE: Duration = Duration::from_secs(5);

/// Shown when a desktop tool is wanted outside a Wayland desktop.
pub const NO_HOST_CLIPBOARD: &str =
    "the daemon was not started inside a Wayland desktop — no host clipboard";

/// A command aimed at the desktop's display, or why there is none.
pub fn command(prog: &str) -> Result<Command, String> {
    let display = crate::clip::env::display().ok_or(NO_HOST_CLIPBOARD)?;
    let mut c = Command::new(prog);
    c.env("WAYLAND_DISPLAY", display).kill_on_drop(true);
    Ok(c)
}

/// Run `prog args…` on the desktop with `stdin`; its stdout.
pub async fn run_with(prog: &str, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>, String> {
    let mut c = command(prog)?;
    let mut child = c
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{prog} is not available here ({e})"))?;
    if let Some(mut i) = child.stdin.take() {
        let _ = i.write_all(stdin).await;
    }
    let out = tokio::time::timeout(CLIP_DEADLINE, child.wait_with_output())
        .await
        .map_err(|_| format!("{prog} took too long"))?
        .map_err(|e| format!("{prog}: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("{prog} failed")
        } else {
            err
        })
    }
}

/// `prog args…` aimed at the desktop's display, awaited for at most `deadline`; its stdout.
pub async fn desktop(prog: &str, args: &[&str], deadline: Duration) -> Result<String, String> {
    let mut c = command(prog)?;
    c.args(args).stdin(Stdio::null());
    let out = tokio::time::timeout(deadline, c.output())
        .await
        .map_err(|_| format!("{prog} took too long"))?
        .map_err(|e| format!("{prog} is not available here ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("{prog} failed")
        } else {
            err.lines().last().unwrap_or(&err).to_string()
        })
    }
}

/// Shown outside a Wayland desktop.
pub const NO_DESKTOP: &str = "the daemon was not started inside a Wayland desktop";

/// The desktop's display, or why there is none.
pub fn display() -> Result<&'static str, String> {
    crate::clip::env::display().ok_or_else(|| NO_DESKTOP.to_string())
}

/// Start `prog args…` on the desktop's display and leave it: an app the user opened outlives the
/// tap that opened it, and the daemon too (its own process group, no pipes back).
pub fn detached(prog: &str, args: &[&str]) -> Result<(), String> {
    let display = display()?;
    tokio::process::Command::new(prog)
        .args(args)
        .env("WAYLAND_DISPLAY", display)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(drop)
        .map_err(|e| format!("{prog} could not start ({e})"))
}
