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
//! | [`stay`] | how long an unlock lasts: each device's own choice |
//! | [`unlock`] | the unlock window: its idle clock, what it outlives, the locks |
//! | [`require`] | whether a device is asked for its passkey at all — its owner's choice |
//! | [`approve`] | unlocking without the passkey: someone at the computer says yes |
//! | [`relay`] | the messages, over the sealed relay link |
//!
//! No new crate: `ring` (already here for the envelope) verifies ES256, and the CBOR is the
//! hand-written [`cbor`].

pub mod approve;
mod approve_notify;
pub mod authdata;
pub mod cbor;
pub mod challenge;
pub mod client_data;
pub mod cose;
pub mod relay;
pub mod require;
pub mod stay;
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

pub(crate) fn refused_with(store: &Store, key: &str, use_it: bool) -> Option<&'static str> {
    // Its owner chose not to ask this device ([`require`]): every passkey door is open to it.
    if !store.required(key) {
        return None;
    }
    // Removed on the computer (`wado passkey remove`, unpairing) while open: shut it now. One
    // look at the folder per use, which is what makes a removal take effect at once. The one
    // window a device without a passkey can have is an approval on the computer ([`approve`]).
    if !store.has(key) {
        if unlock::bare_open(key, use_it) {
            return None;
        }
        unlock::lock_in(store, key);
        return Some(NO_PASSKEY);
    }
    let open = if use_it {
        unlock::touch_in(store, key)
    } else {
        unlock::left_in(store, key).is_some()
    };
    if open { None } else { Some(LOCKED) }
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
        unlock::open_in(&store, &key);
        assert_eq!(refused_with(&store, &key, true), None);
        assert_eq!(refused_with(&store, &key, false), None);
        assert_eq!(refused_with(&store, "someone-else", true), Some(NO_PASSKEY));
        unlock::lock_in(&store, &key);
        assert_eq!(refused_with(&store, &key, false), Some(LOCKED), "re-locked");
        assert!(is_lock(LOCKED) && is_lock(NO_PASSKEY) && !is_lock("no grant"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
