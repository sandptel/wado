//! The passkey messages over the sealed relay link: a challenge, a new passkey, an unlock, a
//! lock, a keepalive — each answered with where the device now stands.

use base64::Engine as _;
use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::passkey::{PasskeyState, REGISTER, UNLOCK};
use wado_protocol::relay::RelayMsg;

use super::{Store, challenge, unlock, verify};

/// The device on the seat, as the envelope proved it.
pub struct Device<'a> {
    pub key: &'a str,
    pub name: &'a str,
    /// It passed the gate.
    pub ok: bool,
}

fn enc(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// Where `key` stands, with `error` said if the last step failed.
pub fn state(store: &Store, key: &str, error: &str) -> PasskeyState {
    let cred = store.get(key);
    let left = unlock::left(key);
    PasskeyState {
        registered: cred.is_some(),
        unlocked: left.is_some(),
        idle_left_s: left.map_or(0, |d| d.as_secs() as u32),
        idle_s: unlock::IDLE.as_secs() as u32,
        error: error.to_string(),
        origin: cred.map(|c| c.origin).unwrap_or_default(),
    }
}

/// Answer one passkey message. Anything else is ignored.
pub async fn handle(msg: RelayMsg, dev: Device<'_>, tx: &mpsc::Sender<String>) {
    let store = Store::default();
    if !dev.ok || dev.key.is_empty() || crate::gate::Gate::default().entry(dev.key).is_none() {
        return;
    }
    let reply = match step(&store, msg, &dev) {
        Some(Reply::Challenge(m)) => m,
        Some(Reply::State(err)) => RelayMsg::PasskeyState {
            state: state(&store, dev.key, &err),
        },
        None => return,
    };
    crate::relay_client::send_relay(tx, &reply).await.ok();
}

enum Reply {
    Challenge(RelayMsg),
    /// The state, with this error.
    State(String),
}

fn step(store: &Store, msg: RelayMsg, dev: &Device<'_>) -> Option<Reply> {
    let key = dev.key;
    Some(match msg {
        RelayMsg::PasskeyGet => Reply::State(String::new()),
        RelayMsg::PasskeyLock => {
            unlock::lock(key);
            Reply::State(String::new())
        }
        RelayMsg::PasskeyKeepalive => {
            unlock::touch(key);
            return None;
        }
        RelayMsg::PasskeyBegin { purpose } => match begin(store, key, &purpose) {
            Ok(m) => Reply::Challenge(m),
            Err(e) => Reply::State(e.into()),
        },
        RelayMsg::PasskeyRegister {
            client_data,
            attestation,
        } => Reply::State(match register(store, key, &client_data, &attestation) {
            Ok(()) => {
                info!(device = dev.name, "passkey: added — unlocked");
                String::new()
            }
            Err(e) => {
                warn!(device = dev.name, "passkey: not added — {e}");
                e.into()
            }
        }),
        RelayMsg::PasskeyUnlock {
            cred_id,
            client_data,
            auth_data,
            signature,
        } => Reply::State(
            match unlock_with(store, key, &cred_id, &client_data, &auth_data, &signature) {
                Ok(()) => {
                    info!(device = dev.name, "passkey: unlocked");
                    String::new()
                }
                Err(e) => {
                    warn!(device = dev.name, "passkey: unlock refused — {e}");
                    e.into()
                }
            },
        ),
        _ => return None,
    })
}

fn begin(store: &Store, key: &str, purpose: &str) -> Result<RelayMsg, &'static str> {
    let have = store.get(key);
    match purpose {
        // Replacing a passkey takes the one already there: whoever holds only the device key
        // must not be able to swap in a passkey of their own. A lost one is removed on the
        // computer (`wado passkey remove`).
        REGISTER if have.is_some() && unlock::left(key).is_none() => Err(
            "This device already has a passkey here — unlock with it first to replace it, or \
             remove it on the computer with `wado passkey remove <device>`.",
        ),
        REGISTER => {
            let user = ring::digest::digest(&ring::digest::SHA256, key.as_bytes());
            Ok(RelayMsg::PasskeyChallenge {
                purpose: REGISTER.into(),
                challenge: enc(&challenge::issue(key, REGISTER)),
                cred_id: String::new(),
                user_id: enc(&user.as_ref()[..16]),
            })
        }
        UNLOCK => {
            let cred = have.ok_or(super::NO_PASSKEY)?;
            Ok(RelayMsg::PasskeyChallenge {
                purpose: UNLOCK.into(),
                challenge: enc(&challenge::issue(key, UNLOCK)),
                cred_id: cred.cred_id,
                user_id: String::new(),
            })
        }
        _ => Err("unknown passkey request"),
    }
}

fn register(store: &Store, key: &str, cd: &str, att: &str) -> Result<(), &'static str> {
    let c = challenge::take(key, REGISTER).ok_or(STALE)?;
    // Checked again, not only at `begin`: the window may have shut in between.
    if store.get(key).is_some() && unlock::left(key).is_none() {
        return Err("Unlock with this device's passkey first to replace it.");
    }
    let cred = verify::register(cd, att, &c)?;
    store
        .put(key, &cred)
        .map_err(|_| "the computer could not save the passkey")?;
    // Making it verified the person just now: that is an unlock.
    unlock::open(key);
    Ok(())
}

fn unlock_with(
    store: &Store,
    key: &str,
    cred_id: &str,
    cd: &str,
    ad: &str,
    sig: &str,
) -> Result<(), &'static str> {
    let c = challenge::take(key, UNLOCK).ok_or(STALE)?;
    let cred = store.get(key).ok_or(super::NO_PASSKEY)?;
    let count = verify::assert(&cred, cred_id, cd, ad, sig, &c)?;
    store.set_count(key, count);
    unlock::open(key);
    Ok(())
}

const STALE: &str = "That took too long — try again.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_passkey_cannot_be_slipped_in_while_locked() {
        let dir = std::env::temp_dir().join(format!("wado-pk-relay-{}", rand::random::<u64>()));
        let store = Store::at(dir.clone());
        let key = format!("dev-{}", rand::random::<u32>());
        assert!(matches!(
            begin(&store, &key, REGISTER),
            Ok(RelayMsg::PasskeyChallenge { .. })
        ));
        assert_eq!(
            begin(&store, &key, UNLOCK).unwrap_err(),
            super::super::NO_PASSKEY
        );
        store
            .put(
                &key,
                &verify::Credential {
                    cred_id: "c".into(),
                    key: "k".into(),
                    count: 0,
                    origin: "https://a.b".into(),
                    rp_id: "a.b".into(),
                    added: 0,
                },
            )
            .unwrap();
        assert!(
            begin(&store, &key, REGISTER).is_err(),
            "locked: no replacing"
        );
        assert!(
            matches!(begin(&store, &key, UNLOCK), Ok(RelayMsg::PasskeyChallenge { cred_id, .. }) if cred_id == "c")
        );
        unlock::open(&key);
        assert!(
            begin(&store, &key, REGISTER).is_ok(),
            "unlocked: may replace"
        );
        unlock::lock(&key);
        assert_eq!(
            register(&store, &key, "x", "y").unwrap_err(),
            "Unlock with this device's passkey first to replace it."
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
