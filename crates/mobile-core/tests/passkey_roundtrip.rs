//! What the Android app's authenticator makes (crate::webauthn + a P-256 key signing as the
//! Keystore does: ECDSA over SHA-256, ASN.1 DER) is accepted by the daemon's own verifier —
//! registration, then an unlock — and a signature over anything else is not.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
};
use wado::passkey::verify;
use wado_mobile_core::webauthn::*;

const ORIGIN: &str = "https://appassets.androidplatform.net";
const RP: &str = "appassets.androidplatform.net";

fn b(v: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(v)
}

/// A capture from a real phone or emulator: the page's create (challenge 01020304) and get
/// (challenge 090909) results, as JSON `[{id, cd, att}, {ad, cd, sig}]` in `WADO_PASSKEY_CAPTURE`.
/// `cargo test -p wado-mobile-core -- --ignored device_capture` (docs/android.md says how to take one).
#[test]
#[ignore]
fn device_capture_is_accepted() {
    let path = std::env::var("WADO_PASSKEY_CAPTURE").expect("WADO_PASSKEY_CAPTURE");
    let mut raw = std::fs::read_to_string(path).unwrap();
    // The capture may be the JSON string itself, quoted once more.
    if raw.trim_start().starts_with('"') {
        raw = serde_json::from_str::<String>(raw.trim()).unwrap();
    }
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let s = |i: usize, k: &str| v[i][k].as_str().unwrap().to_string();
    let cred =
        verify::register(&s(0, "cd"), &s(0, "att"), &[1, 2, 3, 4]).expect("registration accepted");
    assert_eq!(cred.origin, ORIGIN);
    verify::assert(
        &cred,
        &s(0, "id"),
        &s(1, "cd"),
        &s(1, "ad"),
        &s(1, "sig"),
        &[9, 9, 9],
    )
    .expect("unlock accepted");
}

#[test]
fn the_daemon_accepts_the_apps_passkey() {
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
    let key =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    let cred_id = vec![7u8; 16];

    let reg_challenge = [1u8; 32];
    let cd = webauthn_client_data(true, reg_challenge.to_vec(), ORIGIN.into());
    let att = webauthn_attestation(
        RP.into(),
        cred_id.clone(),
        key.public_key().as_ref().to_vec(),
    )
    .unwrap();
    let cred = verify::register(&b(&cd), &b(&att), &reg_challenge).expect("registration accepted");
    assert_eq!(cred.rp_id, RP);
    assert_eq!(cred.origin, ORIGIN);

    let challenge = [2u8; 32];
    let cd = webauthn_client_data(false, challenge.to_vec(), ORIGIN.into());
    let ad = webauthn_auth_data(RP.into(), 0);
    let sig = key
        .sign(&rng, &webauthn_signed(ad.clone(), cd.clone()))
        .unwrap();
    let count = verify::assert(
        &cred,
        &b(&cred_id),
        &b(&cd),
        &b(&ad),
        &b(sig.as_ref()),
        &challenge,
    )
    .expect("unlock accepted");
    assert_eq!(count, 0);

    // Signed for another challenge: refused.
    assert!(verify::assert(
        &cred,
        &b(&cred_id),
        &b(&cd),
        &b(&ad),
        &b(sig.as_ref()),
        &[3u8; 32]
    )
    .is_err());
}
