//! Running a tool on the desktop's display — awaited with a deadline, or started and left to
//! run. Arguments are arguments, never a shell string. (Tools that need no display go through
//! `host::cmd`.)

use std::process::Stdio;
use std::time::Duration;

/// `prog args…` aimed at the desktop's display, awaited for at most `deadline`; its stdout.
pub async fn desktop(prog: &str, args: &[&str], deadline: Duration) -> Result<String, String> {
    let mut c = crate::clip::tool::command(prog)?;
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

/// Start `prog args…` on the desktop's display and leave it: an app the user opened outlives the
/// tap that opened it, and the daemon too (its own process group, no pipes back).
pub fn detached(prog: &str, args: &[&str]) -> Result<(), String> {
    let display =
        crate::clip::env::display().ok_or("the daemon was not started inside a Wayland desktop")?;
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
