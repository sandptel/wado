//! The desktop's clipboard ([`Clipboard`] on Linux): set with `wl-copy`, read with `wl-paste`,
//! followed with `wl-paste --watch` — on the desktop's display.
//!
//! The desktop's own `wl-paste --watch cliphist store` puts what is set here in its history.

use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;
use wado_platform::clip::{Clip, Clipboard};
use wado_platform::{Fut, Res};

use super::on_desktop::{command, run_with};

/// `wl-clipboard` on the desktop's display.
pub struct WlClipboard {
    changes: watch::Sender<u64>,
    /// The watcher runs from the first subscriber on, for the life of the daemon.
    watching: OnceLock<()>,
}

impl WlClipboard {
    /// Offered inside a Wayland desktop.
    pub fn probe() -> Result<WlClipboard, String> {
        command("wl-paste")?;
        Ok(WlClipboard {
            changes: watch::channel(0).0,
            watching: OnceLock::new(),
        })
    }
}

impl Clipboard for WlClipboard {
    fn get(&self) -> Fut<'_, Res<Clip>> {
        Box::pin(current())
    }

    fn set(&self, clip: Clip) -> Fut<'_, Res<()>> {
        Box::pin(async move { set(&clip).await })
    }

    /// Starts the watcher on first use (it needs the runtime). Every viewer and the session sync
    /// share it.
    fn changes(&self) -> watch::Receiver<u64> {
        self.watching.get_or_init(|| {
            let tx = self.changes.clone();
            tokio::spawn(follow(tx));
        });
        self.changes.subscribe()
    }
}

/// Set the host clipboard.
async fn set(clip: &Clip) -> Result<(), String> {
    let mime = if clip.is_text() {
        "text/plain;charset=utf-8"
    } else {
        clip.mime.as_str()
    };
    // stdout and stderr to null: wl-copy forks a child that keeps serving the clipboard, and a
    // pipe it inherited would never reach EOF.
    let mut child = command("wl-copy")?
        .args(["--type", mime])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| format!("wl-copy is not available here ({e})"))?;
    if let Some(mut i) = child.stdin.take() {
        i.write_all(&clip.data)
            .await
            .map_err(|e| format!("wl-copy: {e}"))?;
    }
    let ok = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
        .await
        .map_err(|_| "wl-copy took too long".to_string())?
        .map_err(|e| format!("wl-copy: {e}"))?
        .success();
    if ok {
        Ok(())
    } else {
        Err("wl-copy failed".into())
    }
}

/// What is on the host clipboard now: text if offered, else the first image type.
async fn current() -> Result<Clip, String> {
    let types = run_with("wl-paste", &["--list-types"], b"").await?;
    let types = String::from_utf8_lossy(&types);
    let image = types.lines().find(|t| t.starts_with("image/"));
    let text = types
        .lines()
        .any(|t| t.starts_with("text/plain") || t == "UTF8_STRING");
    let mime = if text {
        "text/plain;charset=utf-8".to_string()
    } else if let Some(i) = image {
        i.to_string()
    } else {
        return Err("nothing we can pass on".into());
    };
    let data = run_with("wl-paste", &["--no-newline", "--type", &mime], b"").await?;
    Ok(Clip {
        mime,
        data: data.into(),
    })
}

/// Watch for the life of the daemon, restarting `wl-paste` if it exits — a desktop restart
/// takes it down with it.
async fn follow(tx: watch::Sender<u64>) {
    loop {
        if let Err(e) = watch_once(&tx).await {
            tracing::warn!("host clipboard watch stopped: {e}");
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

/// `wl-paste --watch echo` prints a line per change.
async fn watch_once(tx: &watch::Sender<u64>) -> Result<(), String> {
    let mut child = command("wl-paste")?
        .args(["--watch", "echo"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("wl-paste: {e}"))?;
    let out = child.stdout.take().ok_or("wl-paste: no stdout")?;
    let mut lines = BufReader::new(out).lines();
    while let Ok(Some(_)) = lines.next_line().await {
        // The desktop's own `cliphist store` runs on the same change; give it a moment so the
        // history read after this tick already has the new entry.
        tokio::time::sleep(Duration::from_millis(250)).await;
        tx.send_modify(|n| *n = n.wrapping_add(1));
    }
    Err("wl-paste exited".into())
}
