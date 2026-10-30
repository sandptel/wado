//! The client handshake against the daemon's own half (`wado::e2e::handshake`), with a real
//! host key in a temp dir: what the phone computes is what the computer accepts.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use wado::e2e::handshake::{finish, reply, Codes};
use wado::e2e::host_key::HostKey;
use wado::gate::Proof as DaemonProof;
use wado_client_core::e2e::handshake::{pin_of, ClientHandshake, HsError, Proof};
use wado_client_core::e2e::seal::{Opener, Sealer};
use wado_client_core::identity::Identity;
use wado_client_core::pins::{Pin, PinFrom};
use wado_protocol::envelope::E2eMsg;

const ROOM: &str = "room-1";

fn host() -> HostKey {
    let d = std::env::temp_dir().join(format!(
        "wado-core-hk-{}-{}",
        std::process::id(),
        rand_u64()
    ));
    HostKey::load_or_create(&d).unwrap()
}

fn rand_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn codes(qr: &[&str], typed: &[&str]) -> Codes {
    Codes {
        qr: qr.iter().map(|s| s.to_string()).collect(),
        typed: typed.iter().map(|s| s.to_string()).collect(),
    }
}

/// One handshake up to the daemon's `finish`: the client's result and the daemon's.
struct Run {
    client: ClientHandshake,
    replied: Result<wado_client_core::e2e::handshake::Replied, HsError>,
    daemon: Option<wado::e2e::handshake::Done>,
}

fn run(h: &HostKey, pin: Option<&Pin>, proof: Proof, live: &Codes) -> Run {
    let me = Identity::ephemeral();
    let (mut client, hello) = ClientHandshake::start(ROOM, &me.client_key);
    let E2eMsg::E2eHello { v, eph } = serde_json::from_str(&hello).unwrap() else {
        panic!("{hello}")
    };
    let (msg, replied_d) = reply(h, ROOM, &me.client_key, v, &eph).unwrap();
    let reply_json = serde_json::to_string(&msg).unwrap();
    let replied = client.on_reply(&reply_json, &me, pin, &proof);
    let daemon = replied.as_ref().ok().map(|r| {
        let E2eMsg::E2eFinish {
            dev_pk,
            sig,
            pair_mac,
            typed_mac,
        } = serde_json::from_str(&r.finish).unwrap()
        else {
            panic!()
        };
        assert_eq!(dev_pk, B64.encode(me.pub_raw));
        finish(replied_d, &dev_pk, &sig, &pair_mac, &typed_mac, live).unwrap()
    });
    Run {
        client,
        replied,
        daemon,
    }
}

#[test]
fn qr_pairing_completes_with_daemon() {
    let h = host();
    let pin = Pin {
        pin: h.pin(),
        from: PinFrom::Qr,
    };
    let r = run(
        &h,
        Some(&pin),
        Proof::Qr("LONGQRCODE".into()),
        &codes(&["OTHER", "LONGQRCODE"], &[]),
    );
    let replied = r.replied.unwrap();
    let mut done = r.daemon.unwrap();
    assert_eq!(done.proof, DaemonProof::Qr("LONGQRCODE".into()));
    assert!(!replied.first_use);
    // The keys agree both ways: the client seals, the daemon opens; and back.
    let frame = Sealer::new(replied.keys.c2d).seal(b"hi");
    let E2eMsg::Sealed { n, c } = serde_json::from_str(&frame).unwrap() else {
        panic!()
    };
    assert_eq!(
        done.opener.open(n, B64.decode(c).unwrap()).as_deref(),
        Some(&b"hi"[..])
    );
    let (n, c) = done.sealer.seal(br#"{"type":"e2e_ok"}"#);
    let frame = serde_json::to_string(&E2eMsg::Sealed {
        n,
        c: B64.encode(c),
    })
    .unwrap();
    assert_eq!(
        Opener::new(replied.keys.d2c).open(&frame).unwrap(),
        br#"{"type":"e2e_ok"}"#
    );
}

#[test]
fn typed_code_yields_sas_words_equal_on_both_ends() {
    let h = host();
    let r = run(
        &h,
        None,
        Proof::Typed(*b"K7QF2MXP"),
        &codes(&[], &["AAAA1111", "K7QF2MXP"]),
    );
    let done = r.daemon.unwrap();
    assert_eq!(done.proof, DaemonProof::Typed("K7QF2MXP".into()));
    let mine = r.client.on_sas(&done.sas_nonce).unwrap();
    assert_eq!(
        wado_protocol::envelope::sas_words(mine),
        wado_protocol::envelope::sas_words(done.sas)
    );
    // A nonce it did not promise (another session's) is refused.
    let other = run(
        &h,
        None,
        Proof::Typed(*b"K7QF2MXP"),
        &codes(&[], &["K7QF2MXP"]),
    );
    assert_eq!(
        r.client.on_sas(&other.daemon.unwrap().sas_nonce),
        Err(HsError::SasMismatch)
    );
}

#[test]
fn pin_mismatch_refuses_and_stops() {
    let h = host();
    let other = host();
    let pin = Pin {
        pin: other.pin(),
        from: PinFrom::Qr,
    };
    let r = run(&h, Some(&pin), Proof::None, &Codes::default());
    // Refused, and no finish frame was made: nothing goes to an impostor.
    assert_eq!(r.replied.unwrap_err(), HsError::PinMismatch);
    assert!(r.daemon.is_none());
}

#[test]
fn bad_host_signature_refuses() {
    let h = host();
    let me = Identity::ephemeral();
    let (mut client, hello) = ClientHandshake::start(ROOM, &me.client_key);
    let E2eMsg::E2eHello { v, eph } = serde_json::from_str(&hello).unwrap() else {
        panic!()
    };
    // The daemon signed a transcript for another device (a relay rewriting the client key).
    let (msg, _) = reply(&h, ROOM, "someone-else", v, &eph).unwrap();
    let json = serde_json::to_string(&msg).unwrap();
    assert_eq!(
        client.on_reply(&json, &me, None, &Proof::None).unwrap_err(),
        HsError::BadSignature
    );
}

#[test]
fn first_use_returns_pin_to_store() {
    let h = host();
    let r = run(&h, None, Proof::None, &Codes::default());
    let replied = r.replied.unwrap();
    assert!(replied.first_use);
    assert_eq!(replied.host_pin, h.pin());
    assert_eq!(replied.host_pin, pin_of(h.public()));
    assert_eq!(r.daemon.unwrap().proof, DaemonProof::None);
}
