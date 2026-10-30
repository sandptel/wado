//! The passkey messages over the sealed relay link: a challenge, a new passkey, an unlock, a
//! lock, a keepalive — each answered with where the device now stands.

use base64::Engine as _;
use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::passkey::{PasskeyState, REGISTER, STAY_DEFAULT, UNLOCK, stay_idle_s};
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
    let left = unlock::left_in(store, key);
    let prefs = store.prefs(key);
    let idle = prefs.idle();
    let required = store.required(key);
    PasskeyState {
        registered: cred.is_some(),
        // Not asked at all: open, as far as every door is concerned.
        unlocked: left.is_some() || !required,
        // Without an idle clock there is nothing to count down: 0, like `idle_s`.
        idle_left_s: left
            .filter(|_| idle.is_some())
            .map_or(0, |d| d.as_secs().min(u64::from(u32::MAX)) as u32),
        idle_s: idle.map_or(0, |d| d.as_secs() as u32),
        error: error.to_string(),
        origin: cred.map(|c| c.origin).unwrap_or_default(),
        stay: if stay_idle_s(&prefs.stay).is_some() {
            prefs.stay
        } else {
            STAY_DEFAULT.into()
        },
        lock_on_leave: prefs.lock_on_leave,
        required,
    }
}

/// Why this link may not use a passkey at all, in the device's words; `None` when it may.
fn not_here(dev: &Device<'_>, store: &Store) -> Option<&'static str> {
    if dev.key.is_empty() {
        Some(
            "This browser sent no device key, so it can't hold a passkey here — allow site storage for wado and reconnect.",
        )
    } else if !dev.ok {
        Some(
            "This device isn't let in on this computer yet — wait for the link to finish connecting, then try again.",
        )
    } else if store.gate().entry(dev.key).is_none() {
        Some(
            "This device isn't on this computer's trust list — pair it again (`wado pair` on the computer).",
        )
    } else {
        None
    }
}

/// Answer one passkey message, always: a request this link cannot serve is answered with a state
/// saying why, never dropped — a dropped one left the phone's button waiting on nothing.
pub async fn handle(msg: RelayMsg, dev: Device<'_>, tx: &mpsc::Sender<String>) {
    handle_in(&Store::default(), msg, dev, tx).await;
}

pub(crate) async fn handle_in(
    store: &Store,
    msg: RelayMsg,
    dev: Device<'_>,
    tx: &mpsc::Sender<String>,
) {
    if let Some(why) = not_here(&dev, store) {
        if matches!(msg, RelayMsg::PasskeyKeepalive) {
            return;
        }
        warn!(device = dev.name, "passkey: refused — {why}");
        let state = PasskeyState {
            error: why.into(),
            ..PasskeyState::default()
        };
        crate::relay_client::send_relay(tx, &RelayMsg::PasskeyState { state })
            .await
            .ok();
        return;
    }
    // The two that wait on a person, or answer on their own: off the link's own loop.
    match msg {
        RelayMsg::PasskeyApprove => {
            super::approve::start(
                store.clone(),
                dev.key.to_string(),
                dev.name.to_string(),
                tx.clone(),
            );
            return;
        }
        RelayMsg::PasskeyApproveCancel => {
            super::approve::cancel(store, dev.key, dev.name, tx).await;
            return;
        }
        _ => {}
    }
    let reply = match step(store, msg, &dev) {
        Some(Reply::Challenge(m)) => m,
        Some(Reply::State(err)) => RelayMsg::PasskeyState {
            state: state(store, dev.key, &err),
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
            unlock::lock_in(store, key);
            info!(device = dev.name, "passkey: locked by the device");
            Reply::State(String::new())
        }
        RelayMsg::PasskeyKeepalive => {
            unlock::touch_in(store, key);
            return None;
        }
        RelayMsg::PasskeyStay {
            stay,
            lock_on_leave,
        } => Reply::State(match set_stay(store, key, &stay, lock_on_leave) {
            Ok(()) => {
                info!(
                    device = dev.name,
                    stay, lock_on_leave, "passkey: unlock lifetime set"
                );
                String::new()
            }
            Err(e) => {
                warn!(device = dev.name, "passkey: lifetime not changed — {e}");
                e.into()
            }
        }),
        RelayMsg::PasskeyRequire { on } => Reply::State(match set_required(store, key, on) {
            Ok(()) => {
                if on {
                    info!(
                        device = dev.name,
                        "passkey: asked for again — turned on by the device"
                    );
                } else {
                    warn!(
                        device = dev.name,
                        "passkey: no longer asked — turned off by the device while unlocked"
                    );
                }
                String::new()
            }
            Err(e) => {
                warn!(device = dev.name, "passkey: requirement not changed — {e}");
                e.into()
            }
        }),
        RelayMsg::PasskeyBegin { purpose } => match begin(store, key, &purpose) {
            Ok(m) => {
                info!(device = dev.name, purpose, "passkey: challenge sent");
                Reply::Challenge(m)
            }
            Err(e) => {
                warn!(device = dev.name, purpose, "passkey: no challenge — {e}");
                Reply::State(e.into())
            }
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

/// While unlocked only: proving the passkey once is what earns the choice. The daemon checks it,
/// not just the card that hides the choice while locked.
fn set_stay(store: &Store, key: &str, stay: &str, lock_on_leave: bool) -> Result<(), &'static str> {
    if !store.has(key) {
        return Err(super::NO_PASSKEY);
    }
    if unlock::left_in(store, key).is_none() {
        return Err(
            "Unlock with your passkey first — only then can you change how long it stays unlocked.",
        );
    }
    if stay_idle_s(stay).is_none() {
        return Err("That isn't one of the choices — update wado on this computer or this phone.");
    }
    let mut p = store.prefs(key);
    p.stay = stay.to_string();
    p.lock_on_leave = lock_on_leave;
    store
        .set_prefs(key, &p)
        .map_err(|_| "the computer could not save that")?;
    unlock::apply(store, key);
    Ok(())
}

/// Stop asking this device for its passkey — only while unlocked, so whoever holds it has proven
/// the passkey (or been approved at the computer) once — or start asking again, at any time.
fn set_required(store: &Store, key: &str, on: bool) -> Result<(), &'static str> {
    if !on && store.required(key) && unlock::left_in(store, key).is_none() {
        return Err("Unlock first — only then can this phone stop asking for its passkey.");
    }
    store
        .set_required(key, on)
        .map_err(|_| "the computer could not save that")?;
    Ok(())
}

fn begin(store: &Store, key: &str, purpose: &str) -> Result<RelayMsg, &'static str> {
    let have = store.get(key);
    match purpose {
        // Replacing a passkey takes the one already there: whoever holds only the device key
        // must not be able to swap in a passkey of their own. A lost one is removed on the
        // computer (`wado passkey remove`).
        REGISTER if have.is_some() && unlock::left_in(store, key).is_none() => Err(
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
    // Checked again, not only at `begin`: the window may have shut in between.
    if store.get(key).is_some() && unlock::left_in(store, key).is_none() {
        return Err("Unlock with this device's passkey first to replace it.");
    }
    let c = challenge::take(key, REGISTER, &signed_over(cd)).ok_or(STALE)?;
    let cred = verify::register(cd, att, &c)?;
    store
        .put(key, &cred)
        .map_err(|_| "the computer could not save the passkey")?;
    // Making it verified the person just now: that is an unlock.
    unlock::open_in(store, key);
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
    let c = challenge::take(key, UNLOCK, &signed_over(cd)).ok_or(STALE)?;
    let cred = store.get(key).ok_or(super::NO_PASSKEY)?;
    let count = verify::assert(&cred, cred_id, cd, ad, sig, &c)?;
    store.set_count(key, count);
    unlock::open_in(store, key);
    Ok(())
}

const STALE: &str = "That took too long — try again.";

/// The challenge the browser says the answer was signed over (empty when unreadable, which
/// matches nothing and reads as stale).
fn signed_over(cd: &str) -> Vec<u8> {
    super::verify::b64(cd)
        .ok()
        .and_then(|raw| super::client_data::signed_over(&raw))
        .unwrap_or_default()
}

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
        unlock::open_in(&store, &key);
        assert!(
            begin(&store, &key, REGISTER).is_ok(),
            "unlocked: may replace"
        );
        unlock::lock_in(&store, &key);
        assert_eq!(
            register(&store, &key, "x", "y").unwrap_err(),
            "Unlock with this device's passkey first to replace it."
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn cred() -> verify::Credential {
        verify::Credential {
            cred_id: "c".into(),
            key: "k".into(),
            count: 0,
            origin: "https://a.b".into(),
            rp_id: "a.b".into(),
            added: 0,
        }
    }

    /// A config folder with `key` trusted (`pinned`: by a pairing code) and a passkey on file.
    fn setup(pinned: bool) -> (std::path::PathBuf, Store, String) {
        let dir = std::env::temp_dir().join(format!("wado-pk-stay-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = format!("dev-{}", rand::random::<u64>());
        let pin = if pinned { "1" } else { "0" };
        std::fs::write(
            dir.join("trusted_clients"),
            format!("{key}\tPhone\tDEVICEPK\t{pin}\n"),
        )
        .unwrap();
        let store = Store::at(dir.join("passkeys"));
        store.put(&key, &cred()).unwrap();
        (dir, store, key)
    }

    const NOT_WHILE_LOCKED: &str =
        "Unlock with your passkey first — only then can you change how long it stays unlocked.";

    #[test]
    fn the_lifetime_changes_only_while_unlocked() {
        let (dir, store, key) = setup(true);
        assert_eq!(
            set_stay(&store, &key, "lock", false).unwrap_err(),
            NOT_WHILE_LOCKED,
            "locked: the daemon refuses, whatever the card shows"
        );
        assert_eq!(store.prefs(&key).stay, "15m", "and nothing was written");
        let s = state(&store, &key, "");
        assert_eq!(
            (s.stay.as_str(), s.idle_s, s.lock_on_leave),
            ("15m", 900, true)
        );
        unlock::open_in(&store, &key);
        assert!(
            set_stay(&store, &key, "someday", true).is_err(),
            "only the four"
        );
        set_stay(&store, &key, "8h", true).unwrap();
        let s = state(&store, &key, "");
        assert!(s.unlocked && s.stay == "8h" && s.idle_s == 8 * 3600 && s.idle_left_s > 900);
        set_stay(&store, &key, "lock", false).unwrap();
        let s = state(&store, &key, "");
        assert!(s.unlocked && s.stay == "lock" && s.idle_s == 0 && !s.lock_on_leave);
        unlock::lock_in(&store, &key);
        assert!(!state(&store, &key, "").unlocked, "Lock now still locks it");
        assert_eq!(
            set_stay(&store, &key, "15m", true).unwrap_err(),
            NOT_WHILE_LOCKED
        );
        assert!(store.remove(&key));
        assert_eq!(
            store.prefs(&key).stay,
            "15m",
            "removing the passkey clears the choice"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn until_i_lock_it_outlives_a_restart_on_a_pinned_line_only() {
        for pinned in [true, false] {
            let (dir, store, key) = setup(pinned);
            unlock::open_in(&store, &key);
            set_stay(&store, &key, "lock", false).unwrap();
            assert_eq!(
                store.prefs(&key).open_since.is_some(),
                pinned,
                "marked on file"
            );
            // A restart: the daemon's memory is gone, the files are not.
            unlock::forget(&key);
            assert_eq!(
                unlock::left_in(&store, &key).is_some(),
                pinned,
                "pinned {pinned}: back after a restart"
            );
            unlock::lock_in(&store, &key);
            unlock::forget(&key);
            assert!(
                unlock::left_in(&store, &key).is_none(),
                "locked before the restart: stays locked"
            );
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_timed_choice_does_not_outlive_a_restart() {
        let (dir, store, key) = setup(true);
        unlock::open_in(&store, &key);
        set_stay(&store, &key, "1h", true).unwrap();
        assert!(store.prefs(&key).open_since.is_none());
        unlock::forget(&key);
        assert!(unlock::left_in(&store, &key).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_removed_passkey_shuts_an_open_window_at_once() {
        let (dir, store, key) = setup(true);
        unlock::open_in(&store, &key);
        set_stay(&store, &key, "lock", false).unwrap();
        assert_eq!(super::super::refused_with(&store, &key, true), None);
        store.remove(&key);
        assert_eq!(
            super::super::refused_with(&store, &key, true),
            Some(super::super::NO_PASSKEY)
        );
        assert!(unlock::left_in(&store, &key).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    async fn answer(store: &Store, dev: Device<'_>, msg: RelayMsg) -> Option<PasskeyState> {
        let (tx, mut rx) = mpsc::channel(4);
        handle_in(store, msg, dev, &tx).await;
        let text = rx.try_recv().ok()?;
        match serde_json::from_str::<RelayMsg>(&text).ok()? {
            RelayMsg::PasskeyState { state } => Some(state),
            _ => None,
        }
    }

    #[tokio::test]
    async fn a_request_is_never_dropped() {
        let (dir, store, key) = setup(true);
        let dev = |key, ok| Device {
            key,
            name: "Phone",
            ok,
        };
        let begin = || RelayMsg::PasskeyBegin {
            purpose: REGISTER.into(),
        };
        for (d, why) in [
            (dev(&key, false), "isn't let in"),
            (dev("", true), "no device key"),
            (dev("stranger", true), "trust list"),
        ] {
            let s = answer(&store, d, begin())
                .await
                .expect("answered, not dropped");
            assert!(s.error.contains(why), "{}", s.error);
        }
        let stay = RelayMsg::PasskeyStay {
            stay: "lock".into(),
            lock_on_leave: false,
        };
        let s = answer(&store, dev(&key, true), stay).await.unwrap();
        assert_eq!(s.error, NOT_WHILE_LOCKED);
        let _ = std::fs::remove_dir_all(dir);
    }
}
