//! The Claude messages over the sealed relay link.
//!
//! Who may do what (Decision Log `2026-10-27`, F3 `2026-10-30`): the cards are metadata, for any
//! device with the `shells` grant, no passkey. Everything that acts — Allow, opening a session's
//! shell, starting one, walking the folders to start it in — also needs a fresh passkey unlock.
//! Deny needs only the grant: it can only stop Claude, never let it do more.

use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::relay::RelayMsg;

use crate::relay_client::send_relay;

pub struct Device<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub ok: bool,
    pub gate: &'a crate::gate::Gate,
}

/// Why `dev` may not see the cards at all.
fn refused(dev: &Device<'_>) -> Option<&'static str> {
    if !dev.ok {
        return Some("this device has not been let in yet");
    }
    crate::shells::access::refused(dev.gate, dev.key)
}

/// The lock, or an error, for a refused step-up.
async fn locked(dev: &Device<'_>, why: &str, tx: &mpsc::Sender<String>) {
    if crate::passkey::is_lock(why) {
        let state = crate::passkey::relay::state(&Default::default(), dev.key, "");
        send_relay(tx, &RelayMsg::PasskeyState { state }).await.ok();
    } else {
        send_relay(
            tx,
            &RelayMsg::SessionError {
                message: why.to_string(),
            },
        )
        .await
        .ok();
    }
}

pub async fn handle(msg: RelayMsg, dev: Device<'_>, tx: &mpsc::Sender<String>) {
    if let Some(why) = refused(&dev) {
        if !matches!(msg, RelayMsg::ClaudeGet) {
            locked(&dev, why, tx).await;
        }
        return;
    }
    // Acting needs the passkey; looking and saying no do not.
    let acts = match &msg {
        RelayMsg::ClaudeAnswer { allow, .. } => *allow,
        RelayMsg::ClaudeOpen { .. }
        | RelayMsg::ClaudeNew { .. }
        | RelayMsg::ClaudeDirsGet { .. } => true,
        _ => false,
    };
    if acts {
        if let Some(why) = crate::passkey::refused(dev.key) {
            locked(&dev, why, tx).await;
            return;
        }
    }
    match msg {
        // Asked on every link-up: someone is here to answer a permission request.
        RelayMsg::ClaudeGet => super::socket::present(true),
        RelayMsg::ClaudeAnswer {
            session,
            ask,
            allow,
        } => {
            let ok = super::board().answer(&session, &ask, allow);
            super::changed();
            info!(device = dev.name, allow, ok, "claude: permission answered");
            if !ok {
                send_relay(
                    tx,
                    &RelayMsg::SessionError {
                        message: "That question was already answered, or Claude stopped waiting."
                            .into(),
                    },
                )
                .await
                .ok();
            }
            return; // the change is pushed by `follow`
        }
        RelayMsg::ClaudeOpen { session } => {
            let card = super::board()
                .sessions
                .iter()
                .find(|s| s.id == session)
                .cloned();
            let opened = match card {
                None => Err("That Claude session is gone.".to_string()),
                Some(c) => match (
                    c.shell.filter(|id| crate::shells::alive(*id)),
                    c.tmux.clone(),
                ) {
                    (Some(id), _) => Ok(id),
                    (None, Some(t)) => {
                        let r = tokio::task::spawn_blocking(move || {
                            super::spawn::attach(&t, &c.cwd, 80, 24)
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        if let (Ok(id), Some(t)) = (&r, &c.tmux) {
                            super::board().shell_opened(t, *id);
                        }
                        r
                    }
                    (None, None) => Err(
                        "This Claude runs outside wado, so it has no shell here — start the next \
                         one from wado (New Claude session) to reach it from your phone."
                            .into(),
                    ),
                },
            };
            reply_shell(opened, tx).await;
            super::changed();
            return;
        }
        RelayMsg::ClaudeNew { dir } => {
            info!(device = dev.name, dir, "claude: new session");
            let r = tokio::task::spawn_blocking(move || super::spawn::start(&dir, 80, 24))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            let r = r.map(|(id, key)| {
                super::board().shell_opened(&key, id);
                id
            });
            reply_shell(r, tx).await;
            return;
        }
        RelayMsg::ClaudeDirsGet { path } => {
            let recent: Vec<String> = {
                let b = super::board();
                let mut r: Vec<String> = Vec::new();
                for s in &b.sessions {
                    if !s.cwd.is_empty() && !r.contains(&s.cwd) {
                        r.push(s.cwd.clone());
                    }
                }
                r
            };
            let dirs = tokio::task::spawn_blocking(move || super::dirs::list(&path, recent))
                .await
                .unwrap_or_default();
            send_relay(tx, &RelayMsg::ClaudeDirs { dirs }).await.ok();
            return;
        }
        _ => return,
    }
    let state = super::state();
    send_relay(tx, &RelayMsg::ClaudeCards { state }).await.ok();
}

async fn reply_shell(r: Result<u32, String>, tx: &mpsc::Sender<String>) {
    let msg = match r {
        Ok(id) => RelayMsg::PtyOpened { id },
        Err(message) => {
            warn!("claude: {message}");
            RelayMsg::SessionError { message }
        }
    };
    send_relay(tx, &msg).await.ok();
}

/// Push the cards to this link's device on every change, while `may()` says it may see them.
pub fn follow(
    tx: mpsc::Sender<String>,
    may: impl Fn() -> bool + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut rx = super::changes();
        struct Gone;
        impl Drop for Gone {
            fn drop(&mut self) {
                super::socket::present(false);
            }
        }
        let _gone = Gone;
        while rx.changed().await.is_ok() {
            let ok = may();
            super::socket::present(ok);
            if !ok {
                continue;
            }
            let state = super::state();
            if send_relay(&tx, &RelayMsg::ClaudeCards { state })
                .await
                .is_err()
            {
                break;
            }
        }
    })
}
