//! Host desktop ⇄ session ⇄ devices: what is copied on one is pasteable on the others.
//!
//! Every copy is noted with the [`hub`](super::hub) first, which says whether it is new (write it
//! to the other side), the echo of a write just made (stop), or a secret. **A secret stays on the
//! side it was copied on**: the desktop's password is not put into the session, the session's not
//! onto the desktop, and neither reaches a device.

use tokio::sync::watch;
use wado_compositor::{CommandSender, CompositorCommand, clipboard::Clip};

use super::hub::{self, Noted, Origin};

/// Put a device's `clip` on the host and in the session. Without a host clipboard the session
/// still gets it, and without a session the host does. A `secret` (the phone said so, and the
/// person sent it on purpose) is set on both with its mark, and kept out of every history.
pub async fn push(
    cmd_tx: Option<&CommandSender>,
    clip: Clip,
    from: Origin,
) -> Result<String, String> {
    wado_protocol::clip::shareable(&clip.mime, clip.data.len())?;
    let hash = hub::hash(&clip.data);
    if hub::get().note(clip.clone(), from) == Noted::Echo {
        // Already the computer's clipboard: nothing to write.
        return Ok(hash);
    }
    if let Some(tx) = cmd_tx {
        let _ = tx.send(CompositorCommand::SetClipboard(clip.clone()));
    }
    if let Ok(host) = crate::platform::get().clipboard() {
        host.set(clip).await?;
    }
    Ok(hash)
}

/// Run both directions for the life of the daemon. Without a host clipboard, the session's
/// copies still reach the hub, and through it the devices.
pub fn start(cmd_tx: CommandSender, mut session: watch::Receiver<Clip>) {
    let host = crate::platform::get().clipboard().ok();
    let to_host = host.clone();
    tokio::spawn(async move {
        while session.changed().await.is_ok() {
            let clip = session.borrow_and_update().clone();
            if hub::get().note(clip.clone(), Origin::Session) != Noted::Shared {
                continue;
            }
            if let Some(h) = &to_host
                && let Err(e) = h.set(clip).await
            {
                tracing::debug!("session clipboard → host: {e}");
            }
        }
    });
    let Some(host) = host else { return };
    tokio::spawn(async move {
        let mut rx = host.changes();
        while rx.changed().await.is_ok() {
            if let Ok(clip) = host.get().await
                && hub::get().note(clip.clone(), Origin::Desktop) == Noted::Shared
            {
                let _ = cmd_tx.send(CompositorCommand::SetClipboard(clip));
            }
        }
    });
}
