//! Answering one opened message on a control link, after [`super::policy`] has said it may be
//! carried and the device's grants allow it — asked afresh on every message, so a grant taken
//! away in `wado tui` stops the next tap.
//!
//! Each answer is a [`RelayMsg`] JSON line on the room's writer (`out`), which seals it.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::pad::PadMsg;
use wado_protocol::relay::RelayMsg;

use super::policy::{self, Need};
use crate::gate::Gate;

/// Who is on the link, as the gate knows them.
#[derive(Clone)]
pub struct Who {
    pub key: String,
    pub name: String,
}

/// What a room needs to answer: the gate, the compositor (for a clipboard push), the room's own
/// picture memory, and its writer.
#[derive(Clone)]
pub struct Ctx {
    pub gate: Gate,
    /// `None` only in tests, which have no compositor; a clipboard push then says so.
    pub cmd_tx: Option<wado_compositor::CommandSender>,
    pub sent: Arc<crate::host::trim::Sent>,
    pub out: mpsc::Sender<String>,
}

/// The device is no longer trusted: the link ends.
pub struct Untrusted;

async fn send(out: &mpsc::Sender<String>, msg: &RelayMsg) {
    if let Ok(t) = serde_json::to_string(msg) {
        let _ = out.send(t).await;
    }
}

/// Answer `text`, one opened message from the device.
pub async fn handle(text: &str, who: &Who, cx: &Ctx) -> Result<(), Untrusted> {
    if !cx.gate.knows(&who.key) {
        info!(device = %who.name, "control link: device no longer trusted — closing");
        return Err(Untrusted);
    }
    let msg = match serde_json::from_str::<RelayMsg>(text) {
        Ok(m) => m,
        Err(e) => {
            warn!(device = %who.name, "control link: unreadable message — {e}");
            return Ok(());
        }
    };
    let grants = cx.gate.grants(&who.key);
    let refused = match policy::need(&msg) {
        Need::Trusted => None,
        Need::Grants(tokens) => policy::missing(&grants, tokens).map(|t| {
            format!(
                "this device may not do that here — on the computer: wado allow {} {t}",
                Gate::typed(&who.name)
            )
        }),
        Need::NotControl => {
            let kind = crate::e2e::link::kind(text).unwrap_or("?").to_string();
            info!(device = %who.name, kind, "control link: refused a non-control message");
            send(
                &cx.out,
                &RelayMsg::SessionError {
                    message: "a widget's link carries the computer's controls only".into(),
                },
            )
            .await;
            return Ok(());
        }
    };
    if let Some(message) = refused {
        info!(device = %who.name, "control link: refused — {message}");
        let answer = match msg {
            RelayMsg::HostDo { .. } => RelayMsg::HostError { message },
            RelayMsg::PadIn {
                msg: PadMsg::Run { id },
            } => RelayMsg::PadOut {
                msg: PadMsg::Ran {
                    id,
                    ok: false,
                    message,
                },
            },
            RelayMsg::PadIn { .. } => RelayMsg::PadOut {
                msg: PadMsg::ActionList {
                    actions: Vec::new(),
                },
            },
            _ => RelayMsg::ClipError { message },
        };
        send(&cx.out, &answer).await;
        return Ok(());
    }
    // Spawned, so a slow tool never holds up the next message.
    let cx = cx.clone();
    let who = who.clone();
    tokio::spawn(async move { run(msg, &who, &cx).await });
    Ok(())
}

async fn run(msg: RelayMsg, who: &Who, cx: &Ctx) {
    match msg {
        RelayMsg::HostGet => state(cx).await,
        RelayMsg::HostDo { action } => {
            info!(device = %who.name, ?action, "control link: host action");
            if let Err(message) = crate::host::act(action).await {
                warn!("control link: host action failed: {message}");
                send(&cx.out, &RelayMsg::HostError { message }).await;
            }
            // A moment for the change to land before reading it back.
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            state(cx).await;
        }
        RelayMsg::ConfigGet => {
            let state = crate::config::link::state_for(&who.key, &cx.gate);
            send(&cx.out, &RelayMsg::ConfigState { state }).await;
        }
        RelayMsg::PadIn {
            msg: PadMsg::Actions,
        } => {
            let actions = crate::actions::list().await;
            send(
                &cx.out,
                &RelayMsg::PadOut {
                    msg: PadMsg::ActionList { actions },
                },
            )
            .await;
        }
        RelayMsg::PadIn {
            msg: PadMsg::Run { id },
        } => {
            let done = crate::actions::run(&id).await;
            info!(device = %who.name, action = id, ok = done.is_ok(), "control link: ran");
            let msg = PadMsg::Ran {
                id,
                ok: done.is_ok(),
                message: done.err().unwrap_or_default(),
            };
            send(&cx.out, &RelayMsg::PadOut { msg }).await;
        }
        clip @ (RelayMsg::ClipList | RelayMsg::ClipGet { .. } | RelayMsg::ClipPush { .. }) => {
            match cx.cmd_tx.clone() {
                Some(cmd_tx) => crate::clip::serve::handle(clip, cmd_tx, cx.out.clone()).await,
                None => {
                    let message = "no compositor here".to_string();
                    send(&cx.out, &RelayMsg::ClipError { message }).await;
                }
            }
        }
        _ => {}
    }
}

async fn state(cx: &Ctx) {
    let mut state = crate::host::state().await;
    cx.sent.trim(&mut state);
    send(&cx.out, &RelayMsg::HostState { state }).await;
}
