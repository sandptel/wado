//! A control link driven end to end against the daemon's half, as the relay would carry it.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use tokio::sync::mpsc;
use wado_protocol::envelope::{E2eMsg, VERSION};
use wado_protocol::relay::RelayMsg;

use super::*;
use crate::e2e::handshake::tests::Client;
use crate::e2e::seal::{Opener, Sealer};

fn gate() -> Gate {
    let dir = std::env::temp_dir().join(format!("wado-ctl-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    let g = Gate::at(dir);
    // The first device trusted is the owner, who may do everything: not the phone under test.
    g.trust("owner-key", "laptop");
    g.trust("phone-key", "phone");
    // Nothing granted to start with — and no test here may grant `host`: a host action that
    // passed would really run, on the machine running the tests.
    g.allow(
        "phone",
        &["host", "clipboard", "shells", "input", "settings"],
        false,
    )
    .unwrap();
    g
}

struct Rig {
    rooms: Rooms,
    sh: Shared,
    relay: mpsc::Receiver<String>,
}

fn rig(g: Gate) -> Rig {
    let (tx, relay) = mpsc::channel(64);
    Rig {
        rooms: Rooms::default(),
        sh: Shared {
            gate: g,
            host_key: Arc::new(HostKey::from_seed(&[7u8; 32])),
            cmd_tx: None,
            relay: tx,
            hub: None,
        },
        relay,
    }
}

impl Rig {
    async fn next(&mut self) -> RelayMsg {
        let t = tokio::time::timeout(Duration::from_secs(5), self.relay.recv())
            .await
            .expect("the daemon answers")
            .expect("open");
        serde_json::from_str(&t).unwrap()
    }
    /// The next frame for the client, unwrapped from its `ctl`.
    async fn to_client(&mut self, room: &str) -> String {
        match self.next().await {
            RelayMsg::Ctl { room_id, text } => {
                assert_eq!(room_id, room);
                text
            }
            other => panic!("not a ctl frame: {other:?}"),
        }
    }
    /// Join as `key` and finish the handshake; the room's keys for the client's side.
    async fn join(
        &mut self,
        room: &str,
        key: &str,
        c: &mut Client,
    ) -> Result<(Sealer, Opener), String> {
        self.rooms
            .open(&self.sh, room.into(), key.into(), "phone".into())
            .await;
        let hello = E2eMsg::E2eHello {
            v: VERSION,
            eph: B64.encode(&c.eph_pub),
        };
        self.rooms
            .frame(&self.sh, room, &serde_json::to_string(&hello).unwrap())
            .await;
        let reply: E2eMsg = serde_json::from_str(&self.to_client(room).await).unwrap();
        if let E2eMsg::E2eFail { reason } = reply {
            return Err(reason);
        }
        let ((dev_pk, sig, pair_mac, typed_mac), c2d, d2c) =
            c.answer(room, key, &reply, None, None);
        let fin = E2eMsg::E2eFinish {
            dev_pk,
            sig,
            pair_mac,
            typed_mac,
        };
        self.rooms
            .frame(&self.sh, room, &serde_json::to_string(&fin).unwrap())
            .await;
        let first = self.to_client(room).await;
        if let Ok(E2eMsg::E2eFail { reason }) = serde_json::from_str::<E2eMsg>(&first) {
            return Err(reason);
        }
        let mut opener = Opener::new(&d2c);
        assert!(open(&mut opener, &first).contains("e2e_ok"), "e2e_ok first");
        Ok((Sealer::new(&c2d), opener))
    }
}

fn open(o: &mut Opener, frame: &str) -> String {
    let Ok(E2eMsg::Sealed { n, c }) = serde_json::from_str(frame) else {
        panic!("not sealed: {frame}")
    };
    String::from_utf8(o.open(n, B64.decode(c).unwrap()).expect("opens")).unwrap()
}

fn seal(s: &mut Sealer, body: &str) -> String {
    let (n, c) = s.seal(body.as_bytes());
    serde_json::to_string(&E2eMsg::Sealed {
        n,
        c: B64.encode(c),
    })
    .unwrap()
}

#[tokio::test]
async fn an_unknown_device_gets_no_room() {
    let mut r = rig(gate());
    r.rooms
        .open(&r.sh, "r1".into(), "stranger".into(), "x".into())
        .await;
    let fail = r.to_client("r1").await;
    assert!(fail.contains("e2e_fail"), "{fail}");
    assert!(matches!(r.next().await, RelayMsg::CtlClose { room_id, .. } if room_id == "r1"));
    assert!(r.rooms.is_empty());
}

#[tokio::test]
async fn a_trusted_device_gets_controls_and_nothing_else() {
    let g = gate();
    let mut r = rig(g.clone());
    let (mut s, mut o) = r.join("r1", "phone-key", &mut Client::new()).await.unwrap();

    // The desktop is never on this link, whatever the device may do on the main one.
    g.allow("phone", &["shells", "input"], true).unwrap();
    for m in [
        r#"{"type":"session_launch","command":"touch /tmp/x"}"#,
        r#"{"type":"pad_in","msg":{"k":"key","key":"esc","mods":[]}}"#,
        r#"{"type":"session_stop"}"#,
    ] {
        r.rooms.frame(&r.sh, "r1", &seal(&mut s, m)).await;
        let a = open(&mut o, &r.to_client("r1").await);
        assert!(
            a.contains("session_error") && a.contains("controls only"),
            "{m} → {a}"
        );
    }

    // Which sessions run is shared, read-only: listed on a device's own link (parallel use).
    r.rooms
        .frame(&r.sh, "r1", &seal(&mut s, r#"{"type":"sessions_request"}"#))
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(a.starts_with(r#"{"type":"sessions""#), "{a}");

    // A control still needs its grant: brightness without `host`, the clipboard without
    // `clipboard`, an action without `host`.
    r.rooms
        .frame(
            &r.sh,
            "r1",
            &seal(
                &mut s,
                r#"{"type":"host_do","action":{"do":"brightness","level":0.4}}"#,
            ),
        )
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(
        a.contains("host_error") && a.contains("wado allow phone host"),
        "{a}"
    );
    r.rooms
        .frame(&r.sh, "r1", &seal(&mut s, r#"{"type":"clip_list"}"#))
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(a.contains("clip_error") && a.contains("clipboard"), "{a}");
    // Following the computer's copies needs it too.
    r.rooms
        .frame(
            &r.sh,
            "r1",
            &seal(&mut s, r#"{"type":"clip_watch","live":true}"#),
        )
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(a.contains("clip_error") && a.contains("clipboard"), "{a}");
    r.rooms
        .frame(
            &r.sh,
            "r1",
            &seal(&mut s, r#"{"type":"pad_in","msg":{"k":"run","id":"lock"}}"#),
        )
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(
        a.contains(r#""k":"ran""#) && a.contains(r#""ok":false"#),
        "{a}"
    );

    // Granted now: the clipboard goes through — with no session and no desktop here, to wado's
    // own history in memory.
    g.allow("phone", &["clipboard"], true).unwrap();
    r.rooms
        .frame(&r.sh, "r1", &seal(&mut s, r#"{"type":"clip_list"}"#))
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(
        a.contains("clip_history") && a.contains(r#""available":true"#),
        "{a}"
    );

    // A push lands (no session needed) and is acknowledged by its hash.
    r.rooms
        .frame(
            &r.sh,
            "r1",
            &seal(
                &mut s,
                r#"{"type":"clip_push","mime":"text/plain","data":"from the widget 41"}"#,
            ),
        )
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    let hash = crate::clip::hub::hash(b"from the widget 41");
    assert!(a.contains("clip_pushed") && a.contains(&hash), "{a}");

    // Too big is refused, and says so.
    let big = "x".repeat(wado_protocol::clip::MAX_TEXT + 1);
    let push = serde_json::json!({"type": "clip_push", "mime": "text/plain", "data": big});
    r.rooms
        .frame(&r.sh, "r1", &seal(&mut s, &push.to_string()))
        .await;
    let a = open(&mut o, &r.to_client("r1").await);
    assert!(a.contains("clip_error") && a.contains("too long"), "{a}");

    // Following, live: a copy on the computer reaches the widget's link in full.
    r.rooms
        .frame(
            &r.sh,
            "r1",
            &seal(&mut s, r#"{"type":"clip_watch","live":true}"#),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    crate::clip::hub::get().note(
        wado_compositor::clipboard::Clip {
            mime: "text/plain".into(),
            data: b"copied on the desktop 41".as_slice().into(),
            secret: false,
        },
        crate::clip::hub::Origin::Desktop,
    );
    let mut got = String::new();
    for _ in 0..6 {
        let a = open(&mut o, &r.to_client("r1").await);
        if a.contains("clip_now") {
            got = a;
            break;
        }
    }
    assert!(
        got.contains("copied on the desktop 41") && got.contains(r#""from":"desktop""#),
        "{got}"
    );
    // The history follows it; nothing else is pending after that.
    while tokio::time::timeout(Duration::from_millis(300), r.relay.recv())
        .await
        .is_ok()
    {}

    // A plaintext injection by the relay is dropped, never answered.
    r.rooms.frame(&r.sh, "r1", r#"{"type":"host_get"}"#).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(300), r.relay.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_device_unpaired_mid_link_is_closed_at_its_next_message() {
    let g = gate();
    let mut r = rig(g.clone());
    let (mut s, _o) = r.join("r1", "phone-key", &mut Client::new()).await.unwrap();
    g.unpair("phone").unwrap();
    r.rooms
        .frame(&r.sh, "r1", &seal(&mut s, r#"{"type":"host_get"}"#))
        .await;
    assert!(matches!(r.next().await, RelayMsg::CtlClose { room_id, .. } if room_id == "r1"));
    assert!(r.rooms.is_empty());
}

#[tokio::test]
async fn a_different_key_for_a_known_device_is_refused() {
    let mut r = rig(gate());
    // The first proof pins the key on file…
    r.join("r1", "phone-key", &mut Client::new()).await.unwrap();
    // …so another key claiming the same device is refused, and its room closed.
    let Err(why) = r.join("r2", "phone-key", &mut Client::new()).await else {
        panic!("a second key got in")
    };
    assert!(why.contains("not the one this computer trusts"), "{why}");
    assert!(matches!(r.next().await, RelayMsg::CtlClose { room_id, .. } if room_id == "r2"));
    r.rooms.close("r1");
    assert!(r.rooms.is_empty());
}
