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
    match super::history::list().await {
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
    let done = match msg {
        RelayMsg::ClipList => Ok(true),
        RelayMsg::ClipGet { id } => match super::history::get(&id).await {
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
        RelayMsg::ClipPin { id, on } => super::history::pin(&id, on).await.map(|()| true),
        RelayMsg::ClipDelete { id } => super::history::delete(&id).await.map(|()| true),
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
