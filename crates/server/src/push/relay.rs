//! The push messages over the sealed relay link: the computer's key, this device's
//! subscription and kinds, and a test.

use tokio::sync::mpsc;
use tracing::info;
use wado_protocol::push::{NOTE_TEST, PushInfo, PushKinds, PushNote};
use wado_protocol::relay::RelayMsg;

use super::subs::{Entry, Subs};

pub struct Device<'a> {
    pub key: &'a str,
    pub name: &'a str,
    pub ok: bool,
}

fn info(key: &str, error: String) -> PushInfo {
    let entry = Subs::default().get(key);
    PushInfo {
        key: super::vapid::Vapid::load_or_make(&super::vapid::default_path())
            .map(|v| v.public())
            .unwrap_or_default(),
        subscribed: entry.is_some(),
        kinds: entry.map_or_else(PushKinds::default, |e| e.kinds),
        error,
    }
}

pub async fn handle(msg: RelayMsg, dev: Device<'_>, tx: &mpsc::Sender<String>) {
    if !dev.ok || dev.key.is_empty() || crate::gate::Gate::default().entry(dev.key).is_none() {
        return;
    }
    let key = dev.key.to_string();
    let subs = Subs::default();
    let error = match msg {
        RelayMsg::PushGet => String::new(),
        RelayMsg::PushSubscribe { mut sub, kinds } => match super::subs::check(&sub) {
            Ok(()) => {
                if sub.kind.is_empty() {
                    sub.kind = wado_protocol::push::SUB_WEB.into();
                }
                info!(device = dev.name, kind = sub.kind, "push: subscribed");
                let e = Entry {
                    device: key.clone(),
                    sub,
                    kinds,
                    added: crate::passkey::now_s(),
                };
                subs.put(&e)
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            }
            Err(e) => e,
        },
        RelayMsg::PushKindsSet { kinds } => match subs.get(&key) {
            Some(mut e) => {
                e.kinds = kinds;
                subs.put(&e)
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            }
            None => "Turn notifications on first.".into(),
        },
        RelayMsg::PushUnsubscribe => {
            subs.remove(&key);
            String::new()
        }
        RelayMsg::PushTest => match subs.get(&key) {
            None => "Turn notifications on first.".into(),
            Some(e) => {
                let (computer, name) = super::computer();
                let note = PushNote {
                    kind: NOTE_TEST.into(),
                    title: format!("Hello from {name}"),
                    body: "Notifications from this computer reach this device.".into(),
                    computer,
                    computer_name: name,
                    tag: "test".into(),
                    at: crate::passkey::now_s(),
                    ..Default::default()
                };
                tokio::task::spawn_blocking(move || {
                    let v = super::vapid::Vapid::load_or_make(&super::vapid::default_path())?;
                    super::send_one(&v, &e, &note)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r)
                .map_or_else(
                    |e| format!("The test didn't go out: {e}"),
                    |c| {
                        if (200..300).contains(&c) {
                            String::new()
                        } else {
                            format!("The push service refused the test ({c}) — turn notifications off and on again.")
                        }
                    },
                )
            }
        },
        _ => return,
    };
    let info = tokio::task::spawn_blocking(move || info(&key, error))
        .await
        .unwrap_or_default();
    crate::relay_client::send_relay(tx, &RelayMsg::PushInfo { info })
        .await
        .ok();
}
