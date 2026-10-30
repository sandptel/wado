//! Web Push from this computer (F3, `2026-10-30`): Claude needs permission, Claude finished, the
//! computer is back online, a big transfer finished — each one a per-device, per-kind switch.
//!
//! Any RFC 8030 endpoint works the same way — a browser's push service for the web app, a
//! UnifiedPush distributor for the Android app — because every payload is RFC 8291 encrypted
//! and VAPID-signed, and the payload is the one [`wado_protocol::push::PushNote`] both render.
//!
//! | file | job |
//! |---|---|
//! | [`vapid`] | this computer's push key and the signed token every push carries |
//! | [`ece`] | the payload encryption (aes128gcm) |
//! | [`subs`] | each device's subscription and the kinds it wants |
//! | [`send`] | one HTTPS POST to a push service, never at this machine itself |
//! | [`online`] | "back online", sent when the daemon gets back to the relay |
//! | [`transfer`] | "transfer finished", from the files log |
//! | [`relay`] | the messages over the relay link |
//!
//! **What it cannot do:** say the computer went *offline* — nothing is left running to say it.
//! Only the relay sees a computer vanish, and it holds no push keys (by design: the relay is
//! signalling only and never sees a device's secrets). So the device is told when it comes
//! back instead, and the app's own badge says offline as soon as it looks.

pub mod ece;
pub mod online;
pub mod relay;
pub mod send;
pub mod subs;
pub mod transfer;
pub mod vapid;

use std::time::Duration;

use tracing::{info, warn};
use wado_protocol::push::{
    ACTION_ALLOW, ACTION_DENY, NOTE_CLAUDE_DONE, NOTE_CLAUDE_PERMISSION, PushAction, PushKinds,
    PushNote,
};

use crate::claude::board::Effect;

/// This computer as a notification names it: its Remote ID and its name.
pub fn computer() -> (String, String) {
    let id = std::fs::read_to_string(crate::remote_id::config_dir().join("remote_id"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let name = std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "Your computer".into());
    (id, name)
}

/// True once per `tag` across every daemon of this machine: the pool all hears the same Claude
/// event, and one notification is enough.
pub fn once(tag: &str) -> bool {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(std::env::temp_dir, std::path::PathBuf::from)
        .join("wado-push-sent");
    let _ = std::fs::create_dir_all(&dir);
    let name: String = ring::digest::digest(&ring::digest::SHA256, tag.as_bytes())
        .as_ref()
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect();
    // Old marks go, so the folder stays small.
    if rand::random::<u8>() < 8 {
        for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let old = f
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(24 * 3600));
            if old {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dir.join(name))
        .is_ok()
}

/// Encrypt `note` for one subscription and send it. The push service's status.
pub fn send_one(v: &vapid::Vapid, e: &subs::Entry, note: &PushNote) -> Result<u16, String> {
    use base64::Engine as _;
    let d = |s: &str| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(s.trim_end_matches('='))
            .map_err(|_| "bad subscription".to_string())
    };
    let json = serde_json::to_vec(note).map_err(|e| e.to_string())?;
    let body = ece::encrypt(&d(&e.sub.p256dh)?, &d(&e.sub.auth)?, &json)?;
    let urgent = note.kind == NOTE_CLAUDE_PERMISSION;
    let mut headers = vec![
        ("Content-Encoding", "aes128gcm".to_string()),
        ("Content-Type", "application/octet-stream".to_string()),
        // A question nobody saw in five minutes has been answered at the desk or timed out.
        ("TTL", if urgent { "300" } else { "86400" }.to_string()),
        (
            "Urgency",
            if urgent { "high" } else { "normal" }.to_string(),
        ),
        (
            "Authorization",
            v.header(&e.sub.endpoint, crate::passkey::now_s())?,
        ),
    ];
    let topic: String = note
        .tag
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if !topic.is_empty() {
        headers.push(("Topic", topic));
    }
    send::post(&e.sub.endpoint, &headers, &body)
}

/// Send `note` to every subscription `to` picks, off the caller's thread. Gone subscriptions
/// (404, 410) are forgotten.
pub fn deliver(note: PushNote, to: impl Fn(&subs::Entry) -> bool + Send + 'static) {
    if cfg!(test) {
        return; // tests never reach a real push service
    }
    std::thread::spawn(move || {
        let store = subs::Subs::default();
        let targets: Vec<_> = store.all().into_iter().filter(|e| to(e)).collect();
        if targets.is_empty() {
            return;
        }
        let v = match vapid::Vapid::load_or_make(&vapid::default_path()) {
            Ok(v) => v,
            Err(e) => return warn!("push: {e}"),
        };
        for e in targets {
            match send_one(&v, &e, &note) {
                Ok(c) if (200..300).contains(&c) => info!(kind = note.kind, "push: sent"),
                Ok(404 | 410) => {
                    info!("push: a device's subscription is gone — forgotten");
                    store.remove(&e.device);
                }
                Ok(c) => warn!(kind = note.kind, "push: the push service said {c}"),
                Err(err) => warn!(kind = note.kind, "push: not sent — {err}"),
            }
        }
    });
}

/// Devices that see Claude's cards (the `shells` grant), with `kind` on.
fn claude_target(kind: fn(&PushKinds) -> bool) -> impl Fn(&subs::Entry) -> bool + Send + 'static {
    move |e| {
        let gate = crate::gate::Gate::default();
        kind(&e.kinds) && crate::shells::access::refused(&gate, &e.device).is_none()
    }
}

/// Does any device want Claude's permission requests pushed? Then a request is worth waiting
/// for even with no device connected.
pub fn wants_claude_asks() -> bool {
    let t = claude_target(|k| k.claude_permission);
    subs::Subs::default().all().iter().any(t)
}

/// A Claude event's notification, if it has one.
pub fn claude(effect: &Effect) {
    let (computer, computer_name) = computer();
    let base = PushNote {
        computer,
        computer_name: computer_name.clone(),
        at: crate::passkey::now_s(),
        ..Default::default()
    };
    match effect {
        Effect::Asked(card, ask) => {
            if !once(&format!("ask-{}", ask.id)) {
                return;
            }
            let note = PushNote {
                kind: NOTE_CLAUDE_PERMISSION.into(),
                title: format!("Claude needs you · {}", card.project),
                body: format!("{}: {}", ask.tool, ask.detail),
                tag: format!("claude-{}", short(&card.id)),
                actions: vec![
                    PushAction {
                        id: ACTION_ALLOW.into(),
                        title: "Allow".into(),
                    },
                    PushAction {
                        id: ACTION_DENY.into(),
                        title: "Deny".into(),
                    },
                ],
                session: card.id.clone(),
                ask: ask.id.clone(),
                ..base
            };
            deliver(note, claude_target(|k| k.claude_permission));
        }
        Effect::Finished(card) => {
            if !once(&format!("done-{}-{}", card.id, card.since)) {
                return;
            }
            let note = PushNote {
                kind: NOTE_CLAUDE_DONE.into(),
                title: format!("Claude finished · {}", card.project),
                body: if card.message.is_empty() {
                    format!("On {computer_name}")
                } else {
                    card.message.clone()
                },
                tag: format!("claude-{}", short(&card.id)),
                session: card.id.clone(),
                ..base
            };
            deliver(note, claude_target(|k| k.claude_done));
        }
        Effect::None => {}
    }
}

fn short(id: &str) -> String {
    id.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_is_once() {
        let tag = format!("t-{}", rand::random::<u64>());
        assert!(once(&tag));
        assert!(!once(&tag), "the second daemon of the pool stays quiet");
    }
}
