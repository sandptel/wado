//! Following the computer's clipboard for one link — a viewer's, or a widget's control room.
//!
//! Each time something new is copied ([`super::hub`]) or the desktop's clipboard changes, the
//! link's [`Listen`] is asked afresh — so a grant taken away in `wado tui`, or the phone turning
//! "Copy from this computer automatically" off, holds from the next copy on — and then:
//!
//! - with `live`, the copy in full ([`RelayMsg::ClipNow`]) — unless this device sent it, which
//!   would be its own copy coming back; one too big for the lane is held ([`super::held`]);
//! - with `watching`, the history again, if it changed.
//!
//! Nothing at all without the `clipboard` grant.

use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use wado_protocol::relay::RelayMsg;

use super::hub::{Copied, Hub};
use super::serve::send;

/// What a link wants now, and may have.
#[derive(Debug, Clone, Default)]
pub struct Listen {
    /// The device on the link.
    pub key: String,
    /// It has the `clipboard` grant.
    pub granted: bool,
    /// It wants the history again whenever it changes.
    pub watching: bool,
    /// It wants every new copy, in full.
    pub live: bool,
}

/// Follow for the life of the link (abort the handle when it ends).
pub fn follow(
    out: mpsc::Sender<String>,
    ask: impl Fn() -> Listen + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(follow_on(
        super::hub::get(),
        super::watch::changes(),
        out,
        ask,
    ))
}

async fn follow_on(
    hub: &'static Hub,
    mut host: watch::Receiver<u64>,
    out: mpsc::Sender<String>,
    ask: impl Fn() -> Listen,
) {
    let mut copies = hub.subscribe();
    let mut host_alive = true;
    let mut last_history = String::new();
    loop {
        let copy: Option<Arc<Copied>> = tokio::select! {
            r = host.changed(), if host_alive => {
                // No desktop clipboard to follow: copies still come through the hub.
                host_alive = r.is_ok();
                None
            }
            r = copies.changed() => {
                if r.is_err() {
                    return;
                }
                copies.borrow_and_update().clone()
            }
        };
        if out.is_closed() {
            return;
        }
        let a = ask();
        if !a.granted {
            continue;
        }
        if let Some(c) = copy
            && a.live
            && c.from.device() != Some(a.key.as_str())
            && wado_protocol::clip::shareable(&c.clip.mime, c.clip.data.len()).is_ok()
        {
            let now =
                super::held::held(&c.clip, "", &c.hash).unwrap_or_else(|| RelayMsg::ClipNow {
                    hash: c.hash.clone(),
                    mime: c.clip.mime.clone(),
                    data: super::data::to_wire(&c.clip),
                    from: c.from.label(),
                    at_ms: c.at_ms,
                });
            send(&out, &now).await;
        }
        if a.watching {
            let h = super::serve::history().await;
            let text = serde_json::to_string(&h).unwrap_or_default();
            if text != last_history {
                let _ = out.send(text.clone()).await;
                last_history = text;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::hub::Origin;
    use std::sync::Mutex;
    use std::time::Duration;
    use wado_compositor::clipboard::Clip;

    fn text(s: &str) -> Clip {
        Clip {
            mime: "text/plain".into(),
            data: s.as_bytes().into(),
            secret: false,
        }
    }

    async fn next(rx: &mut mpsc::Receiver<String>) -> Option<RelayMsg> {
        let t = tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .ok()??;
        serde_json::from_str(&t).ok()
    }

    #[tokio::test]
    async fn copies_reach_a_device_that_asked_and_may() {
        let hub: &'static Hub = Box::leak(Box::default());
        let (out, mut rx) = mpsc::channel(8);
        let want = Arc::new(Mutex::new(Listen {
            key: "phone".into(),
            granted: true,
            watching: false,
            live: true,
        }));
        let w = Arc::clone(&want);
        let (_host_tx, host) = watch::channel(0);
        let task = tokio::spawn(follow_on(hub, host, out, move || w.lock().unwrap().clone()));
        tokio::task::yield_now().await;

        hub.note(text("from the desktop"), Origin::Desktop);
        match next(&mut rx).await {
            Some(RelayMsg::ClipNow {
                hash, data, from, ..
            }) => {
                assert_eq!(data, "from the desktop");
                assert_eq!(from, "desktop");
                assert_eq!(hash, crate::clip::hub::hash(b"from the desktop"));
            }
            other => panic!("expected the copy, got {other:?}"),
        }

        // The phone's own copy, written to the computer, does not come back to it…
        let phone = Origin::Device {
            key: "phone".into(),
            name: "Pixel".into(),
        };
        hub.note(text("from the phone"), phone);
        assert!(next(&mut rx).await.is_none(), "its own copy is not echoed");

        // …and a secret never goes anywhere.
        let mut pw = text("hunter2");
        pw.secret = true;
        hub.note(pw, Origin::Desktop);
        assert!(next(&mut rx).await.is_none(), "a secret is never sent");

        // The grant taken away: the next copy is not sent.
        want.lock().unwrap().granted = false;
        hub.note(text("after the grant went"), Origin::Desktop);
        assert!(next(&mut rx).await.is_none(), "no grant, nothing");

        // Live off ("Copy from this computer automatically" off): no copy either.
        {
            let mut w = want.lock().unwrap();
            w.granted = true;
            w.live = false;
        }
        hub.note(text("live is off"), Origin::Session);
        assert!(next(&mut rx).await.is_none(), "not asked for");
        task.abort();
    }

    #[tokio::test]
    async fn a_picture_too_big_for_the_lane_is_held_and_said() {
        let hub: &'static Hub = Box::leak(Box::default());
        let (out, mut rx) = mpsc::channel(8);
        let (_host_tx, host) = watch::channel(0);
        let task = tokio::spawn(follow_on(hub, host, out, || Listen {
            key: "phone".into(),
            granted: true,
            watching: false,
            live: true,
        }));
        tokio::task::yield_now().await;
        let big = Clip {
            mime: "image/png".into(),
            data: vec![7u8; 2 << 20].into(),
            secret: false,
        };
        hub.note(big, Origin::Desktop);
        match next(&mut rx).await {
            Some(RelayMsg::ClipHeld { hash, bytes, .. }) => {
                assert_eq!(hash, crate::clip::hub::hash(&vec![7u8; 2 << 20]));
                assert_eq!(bytes, 2 << 20);
            }
            other => panic!("expected the note, got {other:?}"),
        }
        task.abort();
    }
}
