//! Running the host's own tools — `wpctl`, `nmcli`, `bluetoothctl` — with a deadline.
//!
//! Arguments are passed as arguments, never through a shell, so an SSID or a device name from a
//! client cannot become a command. Every run is counted ([`spawned`]), so what a poll costs in
//! processes is measured, not guessed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::process::Command;

const DEADLINE: Duration = Duration::from_secs(8);

static SPAWNED: AtomicU64 = AtomicU64::new(0);

/// How many tools [`run`] has started since the daemon began.
pub fn spawned() -> u64 {
    SPAWNED.load(Ordering::Relaxed)
}

/// Run `cmd` to completion where blocking is fine (a probe), killing it past `deadline`. `None`
/// when it could not start or ran too long.
pub fn run_blocking(
    mut cmd: std::process::Command,
    deadline: Duration,
) -> Option<std::process::Output> {
    SPAWNED.fetch_add(1, Ordering::Relaxed);
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let until = std::time::Instant::now() + deadline;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Run `prog args…`; stdout on success, a human message otherwise.
pub async fn run(prog: &str, args: &[&str]) -> Result<String, String> {
    SPAWNED.fetch_add(1, Ordering::Relaxed);
    let out = tokio::time::timeout(
        DEADLINE,
        Command::new(prog).args(args).kill_on_drop(true).output(),
    )
    .await
    .map_err(|_| format!("{prog} took too long"))?
    .map_err(|e| format!("{prog} is not available here ({e})"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        Err(if err.is_empty() {
            format!("{prog} failed")
        } else {
            err.lines().last().unwrap_or(err).to_string()
        })
    }
}
