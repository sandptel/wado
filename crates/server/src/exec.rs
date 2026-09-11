//! Running a shell command in the session and streaming its output back.
//!
//! Separate from `SessionLaunch`, which spawns a GUI application and discards its output.
//! Here the output is the point, so the process is spawned with pipes and drained line by
//! line as it runs rather than collected at the end — a command that takes ten seconds should
//! show its first line immediately, and one that never exits should still show what it has
//! printed so far.
//!
//! No PTY. Commands run non-interactively, so anything that needs a terminal — an editor, a
//! pager, `top` — will not work and is not meant to. GUI applications started here do appear
//! on the stream, because `WAYLAND_DISPLAY` is set process-wide and children inherit it.
//!
//! ponytail: line-buffered pipes, not a PTY. The upgrade path if interactive programs are
//! ever wanted is a real pty (portable-pty or nix::pty) plus a terminal emulator client-side.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// One line of output, and which stream it came from.
pub struct Line {
    pub text: String,
    pub err: bool,
}

/// Run `command` under `sh -c`, streaming each output line into `out`. Resolves with the exit
/// code, or `None` if the process was killed by a signal.
///
/// Output is bounded by the channel the caller provides: a command that floods stdout cannot
/// grow memory here, it just blocks on a full channel, which throttles the reader and
/// therefore the pipe.
pub async fn run(command: &str, out: mpsc::Sender<Line>) -> std::io::Result<Option<i32>> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null()) // no PTY, so nothing can answer a prompt — do not let one hang
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    // Both streams are drained concurrently. Reading them in sequence deadlocks the moment a
    // command fills the pipe buffer of whichever one is not being read.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let out_err = out.clone();
    let t_out = tokio::spawn(async move {
        if let Some(s) = stdout {
            let mut lines = BufReader::new(s).lines();
            while let Ok(Some(text)) = lines.next_line().await {
                if out.send(Line { text, err: false }).await.is_err() {
                    break;
                }
            }
        }
    });
    let t_err = tokio::spawn(async move {
        if let Some(s) = stderr {
            let mut lines = BufReader::new(s).lines();
            while let Ok(Some(text)) = lines.next_line().await {
                if out_err.send(Line { text, err: true }).await.is_err() {
                    break;
                }
            }
        }
    });

    let status = child.wait().await?;
    // After wait, so the last lines written before exit are not cut off.
    let _ = t_out.await;
    let _ = t_err.await;
    Ok(status.code())
}
