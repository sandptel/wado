//! Running the clipboard tools against the desktop's display: arguments as arguments, never a
//! shell, with a deadline.

use std::{process::Stdio, time::Duration};

use tokio::{io::AsyncWriteExt, process::Command};

const DEADLINE: Duration = Duration::from_secs(5);

/// A command aimed at the desktop's display, or why there is none.
pub fn command(prog: &str) -> Result<Command, String> {
    let display = super::env::display()
        .ok_or("the daemon was not started inside a Wayland desktop — no host clipboard")?;
    let mut c = Command::new(prog);
    c.env("WAYLAND_DISPLAY", display).kill_on_drop(true);
    Ok(c)
}

/// Run `prog args…` with `stdin`; its stdout.
pub async fn run(prog: &str, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>, String> {
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
    let out = tokio::time::timeout(DEADLINE, child.wait_with_output())
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
