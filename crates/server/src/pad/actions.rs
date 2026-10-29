//! Actions over the pad link (`crate::actions`): the phone asks what can run here, and runs one
//! by id. Gated on `host` per message, not `input`: a device may have the touchpad without the
//! computer's controls, or the reverse. Neither message makes the pad's kernel devices.
//!
//! A refusal is not fatal — the touchpad, if allowed, carries on:
//! - `Actions` refused → an empty `ActionList`, then a non-fatal `Err` with the grant copy.
//! - `Run` refused → `Ran { ok: false }` with the grant copy.
//!
//! A run is spawned, never queued behind the keyboard: a slow tool must not hold up typing.

use tracing::info;
use wado_protocol::pad::PadMsg;

use super::live::{Live, Peer};

/// `Actions` or `Run`, by either transport. Anything else is not ours and is ignored.
pub fn handle(live: &Live, msg: PadMsg, peer: &Peer) {
    let approved = peer.approved.load(std::sync::atomic::Ordering::SeqCst);
    let refused = super::gate::refused_host(&peer.gate, &peer.who, approved);
    let device = peer.who.name.clone();
    match (msg, refused) {
        (PadMsg::Actions, Some(message)) => {
            info!(device, "pad: actions refused — {message}");
            live.reply(PadMsg::ActionList {
                actions: Vec::new(),
            });
            live.reply(PadMsg::Err {
                message,
                fatal: false,
            });
        }
        (PadMsg::Run { id }, Some(message)) => {
            info!(device, action = id, "pad: run refused — {message}");
            live.reply(PadMsg::Ran {
                id,
                ok: false,
                message,
            });
        }
        (PadMsg::Actions, None) => {
            let live = live.clone();
            tokio::spawn(async move {
                let actions = crate::actions::list().await;
                live.reply(PadMsg::ActionList { actions });
            });
        }
        (PadMsg::Run { id }, None) => {
            let live = live.clone();
            tokio::spawn(async move {
                let done = crate::actions::run(&id).await;
                match &done {
                    Ok(()) => info!(device, action = id, "pad: ran"),
                    Err(e) => info!(device, action = id, "pad: run failed — {e}"),
                }
                live.reply(PadMsg::Ran {
                    id,
                    ok: done.is_ok(),
                    message: done.err().unwrap_or_default(),
                });
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use tokio::sync::mpsc;
    use wado_protocol::relay::RelayMsg;

    use super::*;
    use crate::files::channel::Who;
    use crate::gate::Gate;

    /// `k1` "phone" has `host` only; `k2` "tab" has `input` only.
    fn peer(key: &str, name: &str) -> Peer {
        let dir = std::env::temp_dir().join(format!("wado-pad-act-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let g = Gate::at(dir);
        g.trust("k0", "laptop");
        g.trust("k1", "phone");
        g.trust("k2", "tab");
        g.allow("phone", &["host"], true).unwrap();
        g.allow("phone", &["input"], false).unwrap();
        g.allow("tab", &["input"], true).unwrap();
        g.allow("tab", &["host"], false).unwrap();
        Peer {
            who: Who {
                key: key.into(),
                name: name.into(),
            },
            gate: g,
            approved: Arc::new(AtomicBool::new(true)),
        }
    }

    /// The next message the phone is sent through the relay.
    async fn next(rx: &mut mpsc::Receiver<String>) -> PadMsg {
        let text = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("an answer")
            .expect("open");
        match serde_json::from_str(&text).unwrap() {
            RelayMsg::PadOut { msg } => msg,
            other => panic!("not a pad message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn host_without_input_lists_actions_but_may_not_touch() {
        let (live, phone) = (Live::default(), peer("k1", "phone"));
        let (tx, mut rx) = mpsc::channel(16);
        live.relayed(PadMsg::Actions, &phone, &tx);
        assert!(
            matches!(next(&mut rx).await, PadMsg::ActionList { .. }),
            "answered"
        );
        live.relayed(
            PadMsg::Frame {
                seq: 1,
                t: 0,
                c: vec![(0, 10, 10)],
            },
            &phone,
            &tx,
        );
        match next(&mut rx).await {
            PadMsg::Err { message, fatal } => {
                assert!(message.contains("wado allow phone input"), "{message}");
                assert!(fatal);
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn input_without_host_may_not_run_or_list() {
        let (live, tab) = (Live::default(), peer("k2", "tab"));
        let (tx, mut rx) = mpsc::channel(16);
        live.relayed(
            PadMsg::Run {
                id: "workspace-next".into(),
            },
            &tab,
            &tx,
        );
        assert_eq!(
            next(&mut rx).await,
            PadMsg::Ran {
                id: "workspace-next".into(),
                ok: false,
                message: "this device may not run actions — on the computer: wado allow tab host"
                    .into(),
            }
        );
        live.relayed(PadMsg::Actions, &tab, &tx);
        assert_eq!(next(&mut rx).await, PadMsg::ActionList { actions: vec![] });
        assert!(matches!(
            next(&mut rx).await,
            PadMsg::Err { fatal: false, message } if message.contains("wado allow tab host")
        ));
    }

    #[tokio::test]
    async fn a_failed_run_says_why() {
        let (live, phone) = (Live::default(), peer("k1", "phone"));
        let (tx, mut rx) = mpsc::channel(16);
        live.relayed(
            PadMsg::Run {
                id: "make-coffee".into(),
            },
            &phone,
            &tx,
        );
        assert_eq!(
            next(&mut rx).await,
            PadMsg::Ran {
                id: "make-coffee".into(),
                ok: false,
                message: "unknown action \"make-coffee\"".into(),
            }
        );
    }
}
