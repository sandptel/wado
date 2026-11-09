//! Running the Mac's own command-line tools (`open`, `pmset`, `screencapture`, `osascript`),
//! bounded in time so a hung tool cannot hold a request.

use std::time::Duration;

/// Run `prog args…`, waiting at most `limit`. Its stdout on success, its stderr (or a timeout)
/// as the error.
pub async fn run(prog: &str, args: &[&str], limit: Duration) -> Result<String, String> {
    let child = tokio::process::Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(limit, child).await {
        Err(_) => Err(format!("{prog} took longer than {}s", limit.as_secs())),
        Ok(Err(e)) => Err(format!("{prog}: {e}")),
        Ok(Ok(o)) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(Ok(o)) => {
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(if err.is_empty() {
                format!("{prog} failed ({})", o.status)
            } else {
                err
            })
        }
    }
}

/// Start `prog args…` and let it go: an app being opened outlives the request.
pub fn detached(prog: &str, args: &[&str]) -> Result<(), String> {
    std::process::Command::new(prog)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("{prog}: {e}"))
}
