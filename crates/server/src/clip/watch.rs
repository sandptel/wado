//! Noticing the host clipboard change: `wl-paste --watch echo` prints a line each time it does.
//! Started once per daemon; every viewer and the session sync subscribe to [`changes`].

use std::{process::Stdio, sync::OnceLock, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::watch,
};

static TX: OnceLock<watch::Sender<u64>> = OnceLock::new();

fn tx() -> &'static watch::Sender<u64> {
    TX.get_or_init(|| watch::channel(0).0)
}

/// Ticks once per host clipboard change.
pub fn changes() -> watch::Receiver<u64> {
    tx().subscribe()
}

/// Watch for the life of the daemon, restarting `wl-paste` if it exits — a desktop restart
/// takes it down with it. Nothing to watch outside a Wayland desktop.
pub fn start() {
    if super::env::display().is_none() {
        return;
    }
    tokio::spawn(async {
        loop {
            if let Err(e) = run().await {
                tracing::warn!("host clipboard watch stopped: {e}");
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

async fn run() -> Result<(), String> {
    let mut child = super::tool::command("wl-paste")?
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
        tx().send_modify(|n| *n = n.wrapping_add(1));
    }
    Err("wl-paste exited".into())
}
