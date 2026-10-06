//! Host desktop ⇄ session: what is copied on one side is pasteable on the other.
//!
//! Each side's change is written to the other, which then reports a change of its own — the
//! host's watcher fires on our `wl-copy`. [`fresh`] remembers the last value that crossed
//! either way, so that echo stops at one hop.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::Mutex,
};

use tokio::sync::watch;
use wado_compositor::{CommandSender, CompositorCommand, clipboard::Clip};

static LAST: Mutex<u64> = Mutex::new(0);

/// Not the value that last crossed — and now it is.
fn fresh(clip: &Clip) -> bool {
    if clip.data.is_empty() {
        return false;
    }
    let mut h = DefaultHasher::new();
    (clip.is_text(), &clip.data[..]).hash(&mut h);
    let h = h.finish();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if *last == h {
        return false;
    }
    *last = h;
    true
}

/// Put `clip` on the host and in the session — a viewer's paste. Without a host desktop the
/// session still gets it.
pub async fn push(cmd_tx: &CommandSender, clip: Clip) -> Result<(), String> {
    fresh(&clip);
    let _ = cmd_tx.send(CompositorCommand::SetClipboard(clip.clone()));
    if super::env::display().is_some() {
        super::write::set(&clip).await?;
    }
    Ok(())
}

/// Run both directions for the life of the daemon.
pub fn start(cmd_tx: CommandSender, mut session: watch::Receiver<Clip>) {
    if super::env::display().is_none() {
        return;
    }
    tokio::spawn(async move {
        while session.changed().await.is_ok() {
            let clip = session.borrow_and_update().clone();
            if fresh(&clip)
                && let Err(e) = super::write::set(&clip).await
            {
                tracing::debug!("session clipboard → host: {e}");
            }
        }
    });
    tokio::spawn(async move {
        let mut rx = super::watch::changes();
        while rx.changed().await.is_ok() {
            if let Ok(clip) = super::write::current().await
                && fresh(&clip)
            {
                let _ = cmd_tx.send(CompositorCommand::SetClipboard(clip));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_echo_stops_at_one_hop() {
        let c = Clip {
            mime: "text/plain".into(),
            data: b"x".as_slice().into(),
        };
        assert!(fresh(&c));
        assert!(!fresh(&c), "the same value coming back is the echo");
        let d = Clip {
            mime: "text/plain".into(),
            data: b"y".as_slice().into(),
        };
        assert!(fresh(&d));
    }
}
