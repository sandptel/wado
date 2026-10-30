//! WebAuthn bytes for the app's own passkey authenticator (a copy of crates/mobile-core/src/webauthn.rs,
//! which goes with the old app at the cutover): the Keystore signs on the phone, these build the
//! clientDataJSON, authenticatorData and attestationObject around it.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

/// User present, user verified, attested credential data included.
const UP: u8 = 0x01;
const UV: u8 = 0x04;
const AT: u8 = 0x40;

/// `clientDataJSON` for a `webauthn.create` / `webauthn.get` over `challenge` at `origin`.
#[flutter_rust_bridge::frb(sync)]
pub fn webauthn_client_data(create: bool, challenge: Vec<u8>, origin: String) -> Vec<u8> {
    serde_json::json!({
        "type": if create { "webauthn.create" } else { "webauthn.get" },
        "challenge": URL_SAFE_NO_PAD.encode(&challenge),
        "origin": origin,
        "crossOrigin": false,
    })
    .to_string()
    .into_bytes()
}

fn head(rp_id: &str, flags: u8, count: u32) -> Vec<u8> {
    let mut v = Sha256::digest(rp_id.as_bytes()).to_vec();
    v.push(flags);
    v.extend_from_slice(&count.to_be_bytes());
    v
}

/// authenticatorData for an assertion: the person was present and verified.
#[flutter_rust_bridge::frb(sync)]
pub fn webauthn_auth_data(rp_id: String, count: u32) -> Vec<u8> {
    head(&rp_id, UP | UV, count)
}

/// The SHA-256 a signature covers: `authData ‖ SHA-256(clientDataJSON)`.
#[flutter_rust_bridge::frb(sync)]
pub fn webauthn_signed(auth_data: Vec<u8>, client_data: Vec<u8>) -> Vec<u8> {
    let mut v = auth_data;
    v.extend_from_slice(&Sha256::digest(&client_data));
    v
}

// ── a minimal CBOR writer: just the majors these two structures use ──────────────────────
fn cbor_head(out: &mut Vec<u8>, major: u8, n: u64) {
    let m = major << 5;
    match n {
        0..=23 => out.push(m | n as u8),
        24..=0xff => out.extend([m | 24, n as u8]),
        0x100..=0xffff => {
            out.push(m | 25);
            out.extend((n as u16).to_be_bytes());
        }
        _ => {
            out.push(m | 26);
            out.extend((n as u32).to_be_bytes());
        }
    }
}
fn cbor_int(out: &mut Vec<u8>, i: i64) {
    if i >= 0 {
        cbor_head(out, 0, i as u64)
    } else {
        cbor_head(out, 1, (-1 - i) as u64)
    }
}
fn cbor_bytes(out: &mut Vec<u8>, b: &[u8]) {
    cbor_head(out, 2, b.len() as u64);
    out.extend_from_slice(b);
}
fn cbor_text(out: &mut Vec<u8>, s: &str) {
    cbor_head(out, 3, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

/// The COSE key for a P-256 public key given as `04 ‖ x ‖ y` (65 bytes).
fn cose_p256(point: &[u8]) -> Result<Vec<u8>, String> {
    if point.len() != 65 || point[0] != 4 {
        return Err("not an uncompressed P-256 point".into());
    }
    let mut v = vec![];
    cbor_head(&mut v, 5, 5);
    for (k, val) in [(1, 2), (3, -7), (-1, 1)] {
        cbor_int(&mut v, k);
        cbor_int(&mut v, val);
    }
    cbor_int(&mut v, -2);
    cbor_bytes(&mut v, &point[1..33]);
    cbor_int(&mut v, -3);
    cbor_bytes(&mut v, &point[33..]);
    Ok(v)
}

/// The `attestationObject` of a new passkey: `fmt: none`, and authenticatorData carrying the
/// credential id and its public key (`04 ‖ x ‖ y`).
#[flutter_rust_bridge::frb(sync)]
pub fn webauthn_attestation(
    rp_id: String,
    cred_id: Vec<u8>,
    public_key: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let cose = cose_p256(&public_key)?;
    let mut ad = head(&rp_id, UP | UV | AT, 0);
    ad.extend_from_slice(&[0; 16]); // AAGUID: none
    ad.extend_from_slice(&(cred_id.len() as u16).to_be_bytes());
    ad.extend_from_slice(&cred_id);
    ad.extend_from_slice(&cose);
    let mut v = vec![];
    cbor_head(&mut v, 5, 3);
    cbor_text(&mut v, "fmt");
    cbor_text(&mut v, "none");
    cbor_text(&mut v, "attStmt");
    cbor_head(&mut v, 5, 0);
    cbor_text(&mut v, "authData");
    cbor_bytes(&mut v, &ad);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_data_says_the_ceremony_challenge_and_origin() {
        let cd = webauthn_client_data(false, vec![1, 2, 3], "https://a.b".into());
        let v: serde_json::Value = serde_json::from_slice(&cd).unwrap();
        assert_eq!(v["type"], "webauthn.get");
        assert_eq!(v["challenge"], "AQID");
        assert_eq!(v["origin"], "https://a.b");
    }
}
