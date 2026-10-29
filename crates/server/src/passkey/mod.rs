//! Passkey step-up (Decision Log `2026-10-27`, F3 `2026-10-30`): opening a shell, writing files
//! and the trackpad/keyboard need a fresh unlock with the device's passkey, on top of its grants.
//! No passkey, none of the three — with a plain explanation of how to add one.
//!
//! | file | job |
//! |---|---|
//! | [`cbor`] | just enough CBOR for an attestation object and a COSE key |
//! | [`authdata`] | authenticator data: site hash, flags, counter, the new credential |
//! | [`cose`] | the one key kind accepted: ES256 on P-256 |
//! | [`client_data`] | `clientDataJSON`: ceremony, challenge, origin; and which origins may |
//! | [`verify`] | the registration and assertion checks |
//! | [`store`] | each device's passkey on file, beside the trust list |
//! | [`challenge`] | single-use, two-minute challenges |
//! | [`unlock`] | the seat's unlock window: 15 min idle, lock on background |
//! | [`relay`] | the messages, over the sealed relay link |
//!
//! No new crate: `ring` (already here for the envelope) verifies ES256, and the CBOR is the
//! hand-written [`cbor`].

pub mod authdata;
pub mod cbor;
pub mod challenge;
pub mod client_data;
pub mod cose;
pub mod relay;
pub mod store;
pub mod unlock;
pub mod verify;

pub use store::Store;

/// A device with no passkey here. The client matches the start of this wording
/// (`js/passkey.js`), so change both together.
pub const NO_PASSKEY: &str = "This needs a passkey, and this device has none on this computer \
    yet — add one on the computer's page → Connection & trust → Passkey.";

/// A device with a passkey that has not unlocked lately. Matched by the client like
/// [`NO_PASSKEY`].
pub const LOCKED: &str = "Locked — unlock with this device's passkey first.";

pub(crate) fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Why `key` may not use a passkey-guarded door right now, or `None`. A pass counts as use:
/// it restarts the idle clock. Ask this for what the person *does* (types, touches, writes).
pub fn refused(key: &str) -> Option<&'static str> {
    refused_with(&Store::default(), key, true)
}

/// The same question, without counting it as use — for what the computer sends unasked, like a
/// shell's output.
pub fn refused_quiet(key: &str) -> Option<&'static str> {
    refused_with(&Store::default(), key, false)
}

fn refused_with(store: &Store, key: &str, use_it: bool) -> Option<&'static str> {
    let open = if use_it {
        unlock::touch(key)
    } else {
        unlock::left(key).is_some()
    };
    if open {
        return None;
    }
    // Only now the file: a locked window is the common case only for an idle phone.
    if store.get(key).is_none() {
        return Some(NO_PASSKEY);
    }
    Some(LOCKED)
}

/// Is `why` one of this module's refusals (so the device should be shown the lock)?
pub fn is_lock(why: &str) -> bool {
    why == NO_PASSKEY || why == LOCKED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gated_doors_need_a_passkey_then_a_fresh_unlock() {
        let dir = std::env::temp_dir().join(format!("wado-stepup-{}", rand::random::<u64>()));
        let store = Store::at(dir.clone());
        let key = format!("dev-{}", rand::random::<u32>());
        assert_eq!(refused_with(&store, &key, true), Some(NO_PASSKEY));
        store
            .put(
                &key,
                &verify::Credential {
                    cred_id: "x".into(),
                    key: "y".into(),
                    count: 0,
                    origin: "https://a.b".into(),
                    rp_id: "a.b".into(),
                    added: 0,
                },
            )
            .unwrap();
        assert_eq!(
            refused_with(&store, &key, true),
            Some(LOCKED),
            "has one, not unlocked"
        );
        unlock::open(&key);
        assert_eq!(refused_with(&store, &key, true), None);
        assert_eq!(refused_with(&store, &key, false), None);
        assert_eq!(refused_with(&store, "someone-else", true), Some(NO_PASSKEY));
        unlock::lock(&key);
        assert_eq!(refused_with(&store, &key, false), Some(LOCKED), "re-locked");
        assert!(is_lock(LOCKED) && is_lock(NO_PASSKEY) && !is_lock("no grant"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
