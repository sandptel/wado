//! Answering a viewer's clipboard messages, and following the host clipboard for it. Whether the
//! viewer may is the caller's check (the `clipboard` grant).

use tokio::sync::mpsc;
use wado_compositor::CommandSender;
use wado_protocol::relay::RelayMsg;

async fn send(out: &mpsc::Sender<String>, msg: &RelayMsg) {
    if let Ok(t) = serde_json::to_string(msg) {
        let _ = out.send(t).await;
    }
}

/// The history as the rail shows it, or why there is none.
pub async fn history() -> RelayMsg {
    history_from(crate::platform::get()).await
}

/// The history `p` keeps, as the rail shows it.
pub async fn history_from(p: &wado_platform::Platform) -> RelayMsg {
    let listed = match p.clip_history() {
        Ok(h) => h.list().await,
        Err(absent) => Err(absent.why),
    };
    match listed {
        Ok(entries) => RelayMsg::ClipHistory {
            available: true,
            entries,
            error: None,
        },
        Err(e) => RelayMsg::ClipHistory {
            available: false,
            entries: Vec::new(),
            error: Some(e),
        },
    }
}

/// One of the `Clip*` messages. Others are ignored.
pub async fn handle(msg: RelayMsg, cmd_tx: CommandSender, out: mpsc::Sender<String>) {
    let store = crate::platform::get().clip_history().map_err(|a| a.why);
    let done = match msg {
        RelayMsg::ClipList => Ok(true),
        RelayMsg::ClipGet { id } => match async { store?.get(id.clone()).await }.await {
            Ok(clip) => {
                let msg = RelayMsg::ClipData {
                    id,
                    mime: clip.mime.clone(),
                    data: super::data::to_wire(&clip),
                };
                send(&out, &msg).await;
                Ok(false)
            }
            Err(e) => Err(e),
        },
        // The watcher's tick sends the new history once cliphist has it.
        RelayMsg::ClipPush { mime, data } => match super::data::from_wire(&mime, &data) {
            Ok(clip) => super::sync::push(&cmd_tx, clip).await.map(|()| false),
            Err(e) => Err(e),
        },
        RelayMsg::ClipPin { id, on } => match store {
            Ok(h) => h.pin(id, on).await.map(|()| true),
            Err(e) => Err(e),
        },
        RelayMsg::ClipDelete { id } => match store {
            Ok(h) => h.delete(id).await.map(|()| true),
            Err(e) => Err(e),
        },
        _ => Ok(false),
    };
    match done {
        Ok(true) => send(&out, &history().await).await,
        Ok(false) => {}
        Err(message) => send(&out, &RelayMsg::ClipError { message }).await,
    }
}

/// Send the history again whenever the host clipboard changes, while `allowed` says yes.
pub fn follow(
    out: mpsc::Sender<String>,
    allowed: impl Fn() -> bool + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = super::watch::changes();
        while rx.changed().await.is_ok() {
            if allowed() {
                send(&out, &history().await).await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::Fakes;
    use wado_protocol::{ClipEntry, ClipKind};

    #[tokio::test]
    async fn the_rail_shows_the_history_or_why_there_is_none() {
        let f = Fakes::new();
        let entry = ClipEntry {
            id: "7".into(),
            kind: ClipKind::Text,
            pinned: false,
            preview: "hello".into(),
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
        assert!(!available);
        assert_eq!(error.as_deref(), Some(wado_platform::registry::LEFT_OUT));
    }
}
