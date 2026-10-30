//! Answering a device's clipboard messages. Whether the device may is the caller's check (the
//! `clipboard` grant), made again for every message.

use std::sync::Arc;

use tokio::sync::mpsc;
use wado_compositor::CommandSender;
use wado_platform::clip::ClipHistory;
use wado_protocol::relay::RelayMsg;

use super::hub::{self, Origin};

pub(crate) async fn send(out: &mpsc::Sender<String>, msg: &RelayMsg) {
    if let Ok(t) = serde_json::to_string(msg) {
        let _ = out.send(t).await;
    }
}

/// Who sent a message: the device's key and name.
#[derive(Debug, Clone)]
pub struct Sender {
    pub key: String,
    pub name: String,
}

/// The history this computer keeps: the desktop's `cliphist` if it has one, else wado's own, in
/// memory ([`super::ram`]).
fn store(p: &wado_platform::Platform) -> Arc<dyn ClipHistory> {
    match p.clip_history() {
        Ok(h) => h,
        Err(_) => hub::get().ram(),
    }
}

/// The history as the Clipboard surface shows it, or why there is none.
pub async fn history() -> RelayMsg {
    history_from(crate::platform::get()).await
}

/// The history `p` keeps, as the surface shows it: no secret in it, and each entry with the time
/// and origin wado saw it with.
pub async fn history_from(p: &wado_platform::Platform) -> RelayMsg {
    let hub = hub::get();
    match store(p).list().await {
        Ok(mut entries) => {
            entries.retain(|e| !hub.hides(&e.preview));
            hub.ram().annotate(&mut entries);
            RelayMsg::ClipHistory {
                available: true,
                entries,
                error: None,
            }
        }
        Err(e) => RelayMsg::ClipHistory {
            available: false,
            entries: Vec::new(),
            error: Some(e),
        },
    }
}

/// One of the `Clip*` messages from `who`. Others are ignored. `cmd_tx` is the session's
/// compositor, for a push; without one a push still reaches the desktop.
pub async fn handle(
    msg: RelayMsg,
    cmd_tx: Option<CommandSender>,
    who: Sender,
    out: mpsc::Sender<String>,
) {
    let p = crate::platform::get();
    let done = match msg {
        RelayMsg::ClipList => Ok(true),
        RelayMsg::ClipGet { id } => match store(p).get(id.clone()).await {
            // A secret the history kept anyway (an older cliphist) is not handed out.
            Ok(clip) if hub::get().is_secret(&hub::hash(&clip.data)) => {
                Err("that copy was a password — it is not shared".to_string())
            }
            Ok(clip) => {
                let msg = super::held::held(&clip, &id, "").unwrap_or_else(|| RelayMsg::ClipData {
                    id,
                    mime: clip.mime.clone(),
                    data: super::data::to_wire(&clip),
                });
                send(&out, &msg).await;
                Ok(false)
            }
            Err(e) => Err(e),
        },
        // The follower sends the new history once the desktop's history has it.
        RelayMsg::ClipPush {
            mime,
            data,
            sensitive,
        } => match super::data::from_wire(&mime, &data) {
            Ok(mut clip) => {
                clip.secret = sensitive;
                let from = Origin::Device {
                    key: who.key,
                    name: who.name,
                };
                match super::sync::push(cmd_tx.as_ref(), clip, from).await {
                    Ok(hash) => {
                        send(&out, &RelayMsg::ClipPushed { hash }).await;
                        Ok(false)
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        },
        RelayMsg::ClipPin { id, on } => store(p).pin(id, on).await.map(|()| true),
        RelayMsg::ClipDelete { id } => store(p).delete(id).await.map(|()| true),
        _ => Ok(false),
    };
    match done {
        Ok(true) => send(&out, &history().await).await,
        Ok(false) => {}
        Err(message) => send(&out, &RelayMsg::ClipError { message }).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::Fakes;
    use wado_protocol::{ClipEntry, ClipKind};

    #[tokio::test]
    async fn the_surface_shows_the_history_or_wados_own() {
        let f = Fakes::new();
        let entry = ClipEntry {
            id: "7".into(),
            kind: ClipKind::Text,
            pinned: false,
            preview: "hello".into(),
            at_ms: 0,
            from: String::new(),
        };
        f.history.entries.lock().unwrap().push(entry.clone());
        let RelayMsg::ClipHistory {
            available, entries, ..
        } = history_from(&f.platform()).await
        else {
            panic!("not a history");
        };
        assert!(available);
        assert_eq!(entries, [entry]);

        // No desktop history (no cliphist, or no desktop at all): wado's own, in memory.
        let none = wado_platform::PlatformBuilder::empty()
            .with(f.provider())
            .without(wado_platform::Capability::ClipHistory)
            .probe();
        let RelayMsg::ClipHistory {
            available, error, ..
        } = history_from(&none).await
        else {
            panic!("not a history");
        };
        assert!(available, "the in-memory history stands in");
        assert_eq!(error, None);
    }

    #[tokio::test]
    async fn a_secret_in_the_desktop_history_is_hidden() {
        let f = Fakes::new();
        for (id, text) in [("1", "ordinary"), ("2", "correct horse battery")] {
            f.history.entries.lock().unwrap().push(ClipEntry {
                id: id.into(),
                kind: ClipKind::Text,
                pinned: false,
                preview: text.into(),
                at_ms: 0,
                from: String::new(),
            });
        }
        hub::get().note(
            wado_compositor::clipboard::Clip {
                mime: "text/plain".into(),
                data: b"correct  horse\nbattery".as_slice().into(),
                secret: true,
            },
            Origin::Desktop,
        );
        let RelayMsg::ClipHistory { entries, .. } = history_from(&f.platform()).await else {
            panic!("not a history");
        };
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["1"]);
    }
}
