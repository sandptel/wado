//! The Mac daemon, end to end, from a paired device (`scripts/mac-e2e.mjs` starts a local relay
//! and Wado.app, mints the link, and runs this with `--ignored`). Inputs:
//!
//! - `WADO_E2E_LINK`: a single-use pairing link for the local relay;
//! - `WADO_E2E_CFG`: the daemon's config dir; `WADO_E2E_WADO`: its `wado` binary, to grant.
//!
//! Covers: pairing and the control room; the host saying it is a Mac; the capability states on
//! the wire (the session coming, Wi-Fi not on macOS); the Mac's outputs and media card; the
//! clipboard both ways through the real pasteboard; the touchpad moving the real pointer.
#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use wado_client_core::core::Core;
use wado_client_core::event::CoreEvent;
use wado_client_core::link::state::{LinkState, Room};
use wado_client_core::pair::flow::PairStep;
use wado_client_core::secret::SecretBox;

struct Xor;
impl SecretBox for Xor {
    fn wrap(&self, p: Vec<u8>) -> Result<Vec<u8>, String> {
        Ok(p.into_iter().map(|b| b ^ 0x3c).collect())
    }
    fn unwrap(&self, s: Vec<u8>) -> Result<Vec<u8>, String> {
        self.wrap(s)
    }
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("{k} is set by scripts/mac-e2e.mjs"))
}

fn wait<T>(core: &Core, ms: u64, mut pick: impl FnMut(&CoreEvent) -> Option<T>) -> Option<T> {
    let until = Instant::now() + Duration::from_millis(ms);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            let Ok(ev) = tokio::time::timeout(left, core.next_event()).await else {
                break;
            };
            if let Some(t) = pick(&ev) {
                return Some(t);
            }
        }
        None
    })
}

/// The next message in room `id` whose `type` is `ty`.
fn msg(core: &Core, id: &str, ty: &str, ms: u64) -> Option<Value> {
    wait(core, ms, |e| match e {
        CoreEvent::RoomMsg { id: i, json } if i == id => {
            let v: Value = serde_json::from_str(json).ok()?;
            (v["type"] == ty).then_some(v)
        }
        _ => None,
    })
}

fn send(core: &Core, id: &str, v: Value) {
    assert!(
        core.send_room(id, v.to_string(), false, false),
        "room {id} takes {v}"
    );
}

/// Where the pointer is, read by this process (not the daemon).
fn pointer() -> (f64, f64) {
    let out = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", "ObjC.import('CoreGraphics'); var p = $.CGEventGetLocation($.CGEventCreate(null)); p.x + ',' + p.y"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let (x, y) = s.trim().split_once(',').unwrap();
    (x.parse().unwrap(), y.parse().unwrap())
}

#[test]
#[ignore = "needs a relay and Wado.app: scripts/mac-e2e.mjs"]
fn the_mac_from_a_phone() {
    let dir = PathBuf::from(env("WADO_E2E_CFG")).join("core-mac");
    let _ = std::fs::remove_dir_all(&dir);
    let core = Core::new(dir, Arc::new(Xor), "Mac e2e".into()).unwrap();

    // Pair, then grant what the checks below use.
    let op = core.pair(env("WADO_E2E_LINK"));
    let key = wait(&core, 15_000, |e| match e {
        CoreEvent::Pair {
            op: o,
            step: PairStep::Done { key },
        } if *o == op => Some(Ok(key.clone())),
        CoreEvent::Pair {
            op: o,
            step: PairStep::Failed { why },
        } if *o == op => Some(Err(why.clone())),
        _ => None,
    })
    .expect("a pairing outcome in time")
    .expect("pairs");
    let st = Command::new(env("WADO_E2E_WADO"))
        .args(["allow", "Mac e2e", "host", "clipboard"])
        .env("XDG_CONFIG_HOME", env("WADO_E2E_CFG"))
        .status()
        .unwrap();
    assert!(st.success(), "wado allow");

    let room = core
        .open_room(key.clone(), Room::Control)
        .expect("a control room");
    wait(&core, 8000, |e| match e {
        CoreEvent::RoomState { id, view } if *id == room => {
            matches!(view.state, LinkState::Open { .. }).then_some(())
        }
        _ => None,
    })
    .expect("the control room opens");

    // It says it is a Mac, and what it can do there.
    send(&core, &room, json!({"type": "config_get"}));
    let cfg = msg(&core, &room, "config_state", 5000).expect("config_state");
    let state = &cfg["state"];
    let state = if state.is_null() { &cfg } else { state };
    assert_eq!(state["platform"], "macos", "{cfg}");
    let off = state["host_caps"]["off"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let find = |n: &str| off.iter().find(|o| o["cap"] == n).cloned();
    let session = find("session").expect("session is off here");
    assert_eq!(session["later"], true, "{session}");
    assert!(find("wifi").is_some_and(
        |w| w["later"] == false && w["why"].as_str().unwrap().starts_with("Not on macOS")
    ));
    let on: Vec<String> = serde_json::from_value(state["host_caps"]["on"].clone()).unwrap();
    for n in ["clipboard", "audio", "media", "sys_info", "pty", "trash"] {
        assert!(on.iter().any(|c| c == n), "{n} on: {on:?}");
    }
    eprintln!("caps on: {on:?}");

    // The host: a Mac, its outputs, the media keys card.
    send(&core, &room, json!({"type": "host_get", "ack": 7}));
    let host = msg(&core, &room, "host_state", 5000).expect("host_state");
    let s = &host["state"];
    assert_eq!(s["info"]["os_id"], "macos", "{host}");
    assert!(
        s["audio"]["sinks"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "outputs: {}",
        s["audio"]
    );
    assert!(
        s["media"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["bus"] == "media-keys"),
        "{}",
        s["media"]
    );

    // The clipboard: phone → Mac.
    let mark = format!("wado-e2e-{}", std::process::id());
    send(
        &core,
        &room,
        json!({"type": "clip_push", "mime": "text/plain;charset=utf-8", "data": mark}),
    );
    msg(&core, &room, "clip_pushed", 5000).expect("clip_pushed");
    let pasted = Command::new("/usr/bin/pbpaste").output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&pasted.stdout),
        mark,
        "the Mac's clipboard has it"
    );

    // Mac → phone.
    send(&core, &room, json!({"type": "clip_watch", "live": true}));
    std::thread::sleep(Duration::from_millis(600));
    let back = format!("{mark}-back");
    let mut pb = Command::new("/usr/bin/pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    pb.stdin.take().unwrap().write_all(back.as_bytes()).unwrap();
    pb.wait().unwrap();
    let now = msg(&core, &room, "clip_now", 6000).expect("clip_now after a copy on the Mac");
    assert_eq!(now["data"], back, "{now}");

    // The touchpad moves the real pointer (only when Accessibility is granted to Wado).
    if on.iter().any(|c| c == "touch") {
        let before = pointer();
        send(
            &core,
            &room,
            json!({"type": "pad_in", "msg": {"k": "size", "w": 400, "h": 300}}),
        );
        for (i, x) in [10000u32, 14000, 18000, 22000, 26000].iter().enumerate() {
            send(
                &core,
                &room,
                json!({"type": "pad_in", "msg": {"k": "frame", "seq": i + 1, "t": 16 * (i + 1), "c": [[1, x, 30000]]}}),
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        send(
            &core,
            &room,
            json!({"type": "pad_in", "msg": {"k": "frame", "seq": 9, "t": 400, "c": []}}),
        );
        std::thread::sleep(Duration::from_millis(400));
        let after = pointer();
        eprintln!("pointer {before:?} → {after:?}");
        assert!(
            after.0 > before.0 + 20.0,
            "the pointer moved right: {before:?} → {after:?}"
        );
    } else {
        eprintln!("touch is off (no Accessibility for Wado yet) — the pointer check is skipped");
    }

    core.shutdown();
}
