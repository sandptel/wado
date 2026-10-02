//! Running the host's own tools — `wpctl`, `nmcli`, `bluetoothctl` — with a deadline.
//!
//! Arguments are passed as arguments, never through a shell, so an SSID or a device name from a
//! client cannot become a command.

use std::time::Duration;

use tokio::process::Command;

const DEADLINE: Duration = Duration::from_secs(8);

/// Run `prog args…`; stdout on success, a human message otherwise.
pub async fn run(prog: &str, args: &[&str]) -> Result<String, String> {
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
