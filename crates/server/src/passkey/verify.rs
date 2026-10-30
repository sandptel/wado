//! The two WebAuthn checks: a new passkey (`navigator.credentials.create`, attestation "none")
//! and an assertion with it (`navigator.credentials.get`), both ES256.
//!
//! Attestation is not verified — the daemon asks for none, and trusts the device that paired
//! with a code, not the make of its authenticator. What is checked: the challenge is this
//! daemon's, the origin is the one the passkey was made on, the authenticator signed for that
//! site's id, the person was present **and verified** (PIN, fingerprint, face), the signature,
//! and that the counter never goes backwards.

use base64::Engine as _;
use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

use super::{authdata, cbor, client_data};

/// A passkey on file for one device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Credential {
    /// base64url, as the browser names it.
    pub cred_id: String,
    /// `04 ‖ x ‖ y`, base64url.
    pub key: String,
    /// The last signature counter seen.
    pub count: u32,
    /// The page origin it was made on, and that origin's host as the relying party.
    pub origin: String,
    pub rp_id: String,
    /// Unix seconds.
    #[serde(default)]
    pub added: u64,
}

fn b64(s: &str) -> Result<Vec<u8>, &'static str> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .map_err(|_| "a passkey field is not base64url")
}

fn enc(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn sha256(b: &[u8]) -> [u8; 32] {
    let d = ring::digest::digest(&ring::digest::SHA256, b);
    let mut out = [0; 32];
    out.copy_from_slice(d.as_ref());
    out
}

fn person_checked(flags: u8) -> Result<(), &'static str> {
    if flags & authdata::UP == 0 {
        return Err("the passkey was used without anyone touching it");
    }
    if flags & authdata::UV == 0 {
        return Err(
            "the passkey did not check it was you (no PIN, fingerprint or face) — \
                    use one that does",
        );
    }
    Ok(())
}

/// Check a new passkey made over `challenge`. Every field base64url, as the browser gives it.
pub fn register(
    client_data_b64: &str,
    attestation_b64: &str,
    challenge: &[u8],
) -> Result<Credential, &'static str> {
    let cd = b64(client_data_b64)?;
    let origin = client_data::check(&cd, "webauthn.create", challenge)?;
    let rp_id =
        client_data::rp_id(&origin).ok_or("a passkey can only be made on a secure (https) page")?;
    let (att, _) = cbor::decode(&b64(attestation_b64)?)?;
    let raw = att
        .get("authData")
        .and_then(cbor::Cbor::bytes)
        .ok_or("the new passkey came without its authenticator data")?;
    let ad = authdata::parse(raw)?;
    if ad.rp_id_hash != sha256(rp_id.as_bytes()) {
        return Err("the passkey was made for another site");
    }
    person_checked(ad.flags)?;
    let made = ad.attested.ok_or("the new passkey came without its key")?;
    Ok(Credential {
        cred_id: enc(&made.cred_id),
        key: enc(&made.key),
        count: ad.count,
        origin,
        rp_id,
        added: super::now_s(),
    })
}

/// Check an assertion by `cred` over `challenge`. The new counter on success, to be stored.
pub fn assert(
    cred: &Credential,
    cred_id_b64: &str,
    client_data_b64: &str,
    auth_data_b64: &str,
    signature_b64: &str,
    challenge: &[u8],
) -> Result<u32, &'static str> {
    if b64(cred_id_b64)? != b64(&cred.cred_id)? {
        return Err("that is not the passkey this device registered here");
    }
    let cd = b64(client_data_b64)?;
    let origin = client_data::check(&cd, "webauthn.get", challenge)?;
    if origin != cred.origin {
        return Err("the passkey was used from a different site than it was made on");
    }
    let raw = b64(auth_data_b64)?;
    let ad = authdata::parse(&raw)?;
    if ad.rp_id_hash != sha256(cred.rp_id.as_bytes()) {
        return Err("the passkey signed for another site");
    }
    person_checked(ad.flags)?;
    let mut signed = raw.clone();
    signed.extend_from_slice(&sha256(&cd));
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, b64(&cred.key)?)
        .verify(&signed, &b64(signature_b64)?)
        .map_err(|_| "the passkey's signature does not check out")?;
    // A counter that does not move is a synced passkey (it is always 0); one that goes
    // backwards, or stands still after counting, is a cloned authenticator.
    if (cred.count != 0 || ad.count != 0) && ad.count <= cred.count {
        return Err(
            "this passkey's counter went backwards — it may have been copied; \
                    remove it and add a new one",
        );
    }
    Ok(ad.count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("vectors.json")).unwrap()
    }
    fn s<'a>(v: &'a Value, k: &str) -> &'a str {
        v[k].as_str().unwrap()
    }
    fn bytes(v: &Value, k: &str) -> Vec<u8> {
        b64(s(v, k)).unwrap()
    }
    fn registered(v: &Value, which: &str) -> Credential {
        let r = &v[which];
        register(
            s(r, "client_data"),
            s(r, "attestation"),
            &bytes(v, "register_challenge"),
        )
        .unwrap()
    }
    fn assert_with(
        v: &Value,
        cred: &Credential,
        which: &str,
        challenge: &[u8],
    ) -> Result<u32, &'static str> {
        let a = &v[which];
        assert(
            cred,
            s(a, "cred_id"),
            s(a, "client_data"),
            s(a, "auth_data"),
            s(a, "signature"),
            challenge,
        )
    }

    #[test]
    fn a_webcrypto_passkey_registers_and_unlocks() {
        let v = vectors();
        let cred = registered(&v, "register");
        assert_eq!(cred.origin, s(&v, "origin"));
        assert_eq!(cred.rp_id, s(&v, "rp_id"));
        assert_eq!(cred.cred_id, s(&v["register"], "cred_id"));
        assert_eq!(cred.count, 0);
        assert_eq!(
            assert_with(&v, &cred, "assert", &bytes(&v, "get_challenge")),
            Ok(5)
        );
    }

    #[test]
    fn registration_over_another_challenge_is_refused() {
        let v = vectors();
        let r = &v["register"];
        assert!(register(s(r, "client_data"), s(r, "attestation"), &[0; 32]).is_err());
        assert!(
            register(
                s(r, "client_data"),
                "AAAA",
                &bytes(&v, "register_challenge")
            )
            .is_err()
        );
    }

    #[test]
    fn wrong_challenge_is_refused() {
        let v = vectors();
        let cred = registered(&v, "register");
        assert!(assert_with(&v, &cred, "assert", &[1; 32]).is_err());
        // The registration challenge is not an unlock challenge either.
        assert!(assert_with(&v, &cred, "assert", &bytes(&v, "register_challenge")).is_err());
    }

    #[test]
    fn wrong_origin_is_refused() {
        let v = vectors();
        let mut cred = registered(&v, "register");
        cred.origin = "https://evil.example".into();
        assert_eq!(
            assert_with(&v, &cred, "assert", &bytes(&v, "get_challenge")),
            Err("the passkey was used from a different site than it was made on")
        );
        let mut cred = registered(&v, "register");
        cred.rp_id = "evil.example".into();
        assert_eq!(
            assert_with(&v, &cred, "assert", &bytes(&v, "get_challenge")),
            Err("the passkey signed for another site")
        );
    }

    #[test]
    fn bad_signature_is_refused() {
        let v = vectors();
        let cred = registered(&v, "register");
        let mut a = v.clone();
        let mut sig = bytes(&v["assert"], "signature");
        let last = sig.len() - 1;
        sig[last] ^= 1;
        a["assert"]["signature"] = Value::String(enc(&sig));
        assert!(assert_with(&a, &cred, "assert", &bytes(&v, "get_challenge")).is_err());
        // Someone else's key cannot vouch for this signature.
        let other = registered(&v, "synced_register");
        let mut c2 = cred.clone();
        c2.key = other.key;
        assert!(assert_with(&v, &c2, "assert", &bytes(&v, "get_challenge")).is_err());
        // Nor does a different credential id pass for this one.
        let mut a = v.clone();
        a["assert"]["cred_id"] = v["synced_assert"]["cred_id"].clone();
        assert!(assert_with(&a, &cred, "assert", &bytes(&v, "get_challenge")).is_err());
    }

    #[test]
    fn counter_rollback_is_refused_but_a_synced_zero_is_not() {
        let v = vectors();
        let mut cred = registered(&v, "register");
        cred.count = 5;
        assert!(
            assert_with(&v, &cred, "assert", &bytes(&v, "get_challenge"))
                .unwrap_err()
                .contains("backwards"),
            "5 again after 5"
        );
        cred.count = 9;
        assert!(assert_with(&v, &cred, "assert", &bytes(&v, "get_challenge")).is_err());
        let synced = registered(&v, "synced_register");
        assert_eq!(
            assert_with(&v, &synced, "synced_assert", &bytes(&v, "get_challenge")),
            Ok(0)
        );
    }

    #[test]
    fn a_passkey_that_did_not_verify_the_person_is_refused() {
        let v = vectors();
        let cred = registered(&v, "register");
        let mut ad = bytes(&v["assert"], "auth_data");
        ad[32] &= !authdata::UV;
        let mut a = v.clone();
        a["assert"]["auth_data"] = Value::String(enc(&ad));
        assert!(
            assert_with(&a, &cred, "assert", &bytes(&v, "get_challenge"))
                .unwrap_err()
                .contains("check it was you")
        );
    }
}
