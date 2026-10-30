//! The session's state, pushed to a joined room the way the seat's viewer gets it
//! (`relay_client`'s per-connection tasks): the windows, whether an app wants text, the frame
//! divisor, what left the pump, the apps' notifications, the session clipboard and an open menu.
//! State, marked changed at the start, so a device joining mid-session is told the current answer.
//!
//! Not forwarded to a joined room: the host's clipboard history, its shells and its files — those
//! are the device's own link's, with its own grants.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use wado_protocol::relay::RelayMsg;

use super::Hub;
use crate::session::membership::Origin;

async fn send(out: &mpsc::Sender<String>, msg: &RelayMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(t) => out.send(t).await.is_ok(),
        Err(_) => true,
    }
}

/// Start them; they end when `out` closes, or when the handles are aborted.
pub fn spawn(hub: &Arc<Hub>, origin: Origin, out: mpsc::Sender<String>) -> Vec<JoinHandle<()>> {
    let mut tasks = Vec::new();

    // Text input: only the device that last touched the session raises its keyboard.
    {
        let mut rx = hub.feeds.text_input.clone();
        let mut last = hub.last_input.subscribe();
        let (hub, out) = (Arc::clone(hub), out.clone());
        tasks.push(tokio::spawn(async move {
            rx.mark_changed();
            let mut sent = None;
            loop {
                tokio::select! {
                    r = rx.changed() => if r.is_err() { break },
                    r = last.changed() => if r.is_err() { break },
                }
                let active = *rx.borrow_and_update() && hub.keyboard_for(origin);
                last.borrow_and_update();
                if sent != Some(active) {
                    sent = Some(active);
                    if !send(&out, &RelayMsg::TextInput { active }).await {
                        break;
                    }
                }
            }
        }));
    }
    {
        let mut rx = hub.feeds.shedding.clone();
        let out = out.clone();
        tasks.push(tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let divisor = *rx.borrow_and_update();
                if !send(&out, &RelayMsg::Shedding { divisor }).await {
                    break;
                }
            }
        }));
    }
    {
        let mut rx = hub.feeds.sent_kbps.clone();
        let out = out.clone();
        tasks.push(tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let kbps = *rx.borrow_and_update();
                if kbps > 0 && !send(&out, &RelayMsg::SentKbps { kbps }).await {
                    break;
                }
            }
        }));
    }
    {
        let mut rx = hub.feeds.windows.clone();
        let out = out.clone();
        tasks.push(tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let list = rx.borrow_and_update().clone();
                let msg = RelayMsg::Windows {
                    windows: list.windows,
                    workspace: list.workspace,
                };
                if !send(&out, &msg).await {
                    break;
                }
            }
        }));
    }
    {
        let mut rx = hub.feeds.menu.clone();
        let out = out.clone();
        let a11y = Arc::clone(&hub.feeds.a11y);
        tasks.push(tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let spot = rx.borrow_and_update().clone();
                let menu = match &spot {
                    Some(spot) => Some(crate::menu_sheet::read(&a11y, spot).await),
                    None => None,
                };
                if !send(&out, &RelayMsg::Menu { menu }).await {
                    break;
                }
            }
        }));
    }
    // The session clipboard, on change only (not what was copied an hour ago).
    {
        let mut rx = hub.feeds.clipboard.clone();
        let out = out.clone();
        tasks.push(tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let clip = rx.borrow_and_update().clone();
                if !clip.is_text() {
                    continue;
                }
                let text = String::from_utf8_lossy(&clip.data).into_owned();
                if !send(&out, &RelayMsg::Clipboard { text }).await {
                    break;
                }
            }
        }));
    }
    {
        let mut rx = crate::notify::events();
        let out = out.clone();
        tasks.push(tokio::spawn(async move {
            use crate::notify::Event;
            loop {
                let msg = match rx.recv().await {
                    Ok(Event::Shown {
                        id,
                        app,
                        summary,
                        body,
                    }) => RelayMsg::Notification {
                        id,
                        app,
                        summary,
                        body,
                    },
                    Ok(Event::Closed { id }) => RelayMsg::NotificationClosed { id },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                if !send(&out, &msg).await {
                    break;
                }
            }
        }));
    }
    tasks
}
