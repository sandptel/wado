//! `/join/:remote_id?control=1` — a seatless room (a phone's widget, today), joined **without a
//! seat**: see [`crate::controls`].
//!
//! In order: rate limit → park until a daemon of the pool is there → refuse if none of them
//! serves these rooms (an older daemon: the client then shows what it last knew) → tell the
//! daemon (`ctl_open`) → `join_accepted` → carry frames both ways, wrapped by room, until either
//! side goes. Nothing here claims, holds, frees or takes over a seat, so such a room can never
//! stand in the way of a device that wants the desktop.
//!
//! **What a room may carry is the daemon's business, not the relay's.** The relay reads only the
//! room id; today's daemon serves controls only on these rooms (`crate::control` in the daemon).
//!
//! `strict=1` with `instance=` (cap `room_strict`): exactly that daemon of the pool — a client
//! rejoining the session that daemon runs must not land on another. Missing, it parks ("that
//! session's computer is reconnecting") until it is back; there is no fallback.

use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use uuid::Uuid;
use wado_protocol::relay_wire::{display_remote_id, normalize_remote_id, WireMsg, WIRE_VERSION};

use crate::lane::Lane;
use crate::registry::Instance;
use crate::signaling::{
    caps, is_pong, is_relay_ping, send_deny, send_wire, AbortOnDrop, KEEPALIVE, SILENCE,
};
use crate::AppState;

/// The refusal when no daemon of the pool serves control links.
pub const NO_CONTROL: &str =
    "this computer's wado is older than live widgets — update it to see live values here";

/// Who is asking, as the join query said.
pub struct Asker {
    pub wanted: Option<String>,
    pub client_key: String,
    pub name: String,
    /// Only `wanted` will do (`strict=1`).
    pub strict: bool,
}

/// The parked wording for a strict join whose daemon is away.
pub const STRICT_AWAY: &str = "that session's computer is reconnecting — waiting for it";

/// What a join does with the pool as it is now.
pub enum Decision<'a> {
    Go(&'a Instance),
    /// Wait on the registry, saying this.
    Park(&'static str),
    /// No daemon here serves seatless rooms.
    Refuse,
}

/// Decide: go to a daemon, park, or refuse. A strict join goes to its own daemon or parks for
/// it; any other goes where [`pick`] says, parks only while the pool is empty.
pub fn decide<'a>(pool: &'a [Instance], wanted: Option<&str>, strict: bool) -> Decision<'a> {
    if strict {
        if let Some(w) = wanted {
            return match pool.iter().find(|i| i.instance_id == w) {
                Some(i) if i.control => Decision::Go(i),
                Some(_) => Decision::Refuse,
                None => Decision::Park(STRICT_AWAY),
            };
        }
    }
    if pool.is_empty() {
        return Decision::Park(
            "no computer is online with this Remote ID yet — waiting for it to start",
        );
    }
    match pick(pool, wanted) {
        Some(i) => Decision::Go(i),
        None => Decision::Refuse,
    }
}

/// The daemon a control link goes to: the one this device used last if it serves control
/// links, else the first that does. Every daemon of a pool runs on the same computer, so any of
/// them reports the same state.
pub fn pick<'a>(pool: &'a [Instance], wanted: Option<&str>) -> Option<&'a Instance> {
    let serves = |i: &&Instance| i.control;
    wanted
        .and_then(|w| pool.iter().filter(serves).find(|i| i.instance_id == w))
        .or_else(|| pool.iter().find(serves))
}

/// Wrap a client frame for the daemon.
pub fn wrap(room_id: &str, text: &str) -> Option<String> {
    serde_json::to_string(&WireMsg::Ctl {
        room_id: room_id.to_string(),
        text: text.to_string(),
    })
    .ok()
}

pub async fn control_loop(
    socket: WebSocket,
    remote_id: String,
    a: Asker,
    peer: String,
    state: AppState,
) {
    let remote_id = normalize_remote_id(&remote_id);
    let (mut ws_tx, mut ws_rx) = socket.split();
    let rid = display_remote_id(&remote_id);
    let label = if a.name.is_empty() {
        format!("control {peer}")
    } else {
        format!("control {} ({peer})", a.name)
    };

    // ── 1. Rate limit ────────────────────────────────────────────────────────
    if let Err(wait) = state.limiter.take(&peer) {
        let secs = wait.as_secs().max(1);
        send_deny(
            &mut ws_tx,
            &format!("too many connection attempts — try again in {secs} s"),
            false,
            wait.as_millis() as u64,
        )
        .await;
        return;
    }

    // ── 2. Park until a daemon is there ──────────────────────────────────────
    let mut ping = tokio::time::interval(KEEPALIVE);
    ping.tick().await;
    let mut changed = state.registry.watch();
    let mut told = "";
    let instance = loop {
        changed.borrow_and_update();
        let pool = state.registry.instances_for(&remote_id);
        let reason = match decide(&pool, a.wanted.as_deref(), a.strict) {
            Decision::Go(i) => break i.clone(),
            Decision::Refuse => {
                info!(client = %label, remote_id = %rid, "control join refused: no daemon serves seatless rooms");
                send_deny(&mut ws_tx, NO_CONTROL, false, 0).await;
                return;
            }
            Decision::Park(reason) => reason,
        };
        if told != reason {
            let msg = WireMsg::Waiting {
                reason: reason.into(),
                ms_left: 0,
            };
            if !send_wire(&mut ws_tx, &msg).await {
                return;
            }
            told = reason;
        }
        tokio::select! {
            _ = changed.changed() => {}
            _ = ping.tick() => {
                if !send_wire(&mut ws_tx, &WireMsg::Ping).await {
                    return;
                }
            }
            f = ws_rx.next() => match f {
                Some(Ok(Message::Text(_))) | Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {}
                _ => return,
            },
        }
    };

    // ── 3. Open, and accept ──────────────────────────────────────────────────
    let room_id = Uuid::new_v4().to_string();
    let instance_id = instance.instance_id.clone();
    let daemon = instance.inbox_tx.clone();
    let (inbox_tx, mut inbox_rx) = mpsc::channel::<String>(128);
    let mut ended = state
        .controls
        .open(&room_id, &instance_id, inbox_tx.clone());
    let opened = WireMsg::CtlOpen {
        room_id: room_id.clone(),
        client_addr: peer.clone(),
        client_key: a.client_key.clone(),
        client_name: a.name.clone(),
    };
    let accepted = WireMsg::JoinAccepted {
        remote_id: remote_id.clone(),
        room_id: room_id.clone(),
        instance_id: instance_id.clone(),
        pool_size: state.registry.instances_for(&remote_id).len(),
        pool_busy: 0,
        assignment: "control".into(),
        boot_id: instance.boot_id.clone(),
        relay_v: WIRE_VERSION,
        caps: caps(),
    };
    let sent = match serde_json::to_string(&opened) {
        Ok(t) => daemon.send(t).await.is_ok(),
        Err(_) => false,
    };
    if !sent || !send_wire(&mut ws_tx, &accepted).await {
        state.controls.remove(&room_id);
        return;
    }
    info!(client = %label, remote_id = %rid, instance = %instance_id, "control link opened — no seat");

    // ── 4. Pump ──────────────────────────────────────────────────────────────
    let ping_tx = inbox_tx.clone();
    drop(inbox_tx);
    let keepalive = AbortOnDrop(tokio::spawn(async move {
        loop {
            ping.tick().await;
            let Ok(text) = serde_json::to_string(&WireMsg::Ping) else {
                break;
            };
            if ping_tx.send(text).await.is_err() {
                break;
            }
        }
    }));
    // The lane's cap applies to a seatless link exactly as to a seat (`crate::lane`).
    let mut down = Lane::new(format!("{label} ← its computer"));
    let mut up = Lane::new(format!("{label} → its computer"));
    let fwd = AbortOnDrop(tokio::spawn(async move {
        while let Some(text) = inbox_rx.recv().await {
            if !is_relay_ping(&text) {
                down.admit(text.len()).await;
            }
            if ws_tx.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    }));
    let mut last_heard = Instant::now();
    let mut check = tokio::time::interval(Duration::from_secs(5));
    let mut by_daemon = false;
    loop {
        let frame = tokio::select! {
            f = ws_rx.next() => f,
            _ = daemon.closed() => break,
            _ = &mut ended => {
                by_daemon = true;
                break;
            }
            _ = check.tick() => {
                if last_heard.elapsed() > SILENCE {
                    warn!(client = %label, "control client silent for {SILENCE:?} — closing");
                    break;
                }
                continue;
            }
        };
        last_heard = Instant::now();
        match frame {
            Some(Ok(Message::Text(text))) => {
                if is_pong(&text) {
                    continue;
                }
                let Some(w) = wrap(&room_id, &text) else {
                    continue;
                };
                up.admit(text.len()).await;
                if daemon.send(w).await.is_err() {
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            Some(Ok(_)) => {}
        }
    }

    // ── 5. Cleanup ───────────────────────────────────────────────────────────
    // Ended by the daemon: let what it said last (its refusal) reach the client first.
    drop(keepalive);
    state.controls.remove(&room_id);
    if by_daemon {
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            let mut f = fwd;
            let _ = (&mut f.0).await;
        })
        .await;
    } else {
        drop(fwd);
        if let Ok(t) = serde_json::to_string(&WireMsg::CtlClose {
            room_id: room_id.clone(),
            reason: String::new(),
        }) {
            let _ = daemon.send(t).await;
        }
    }
    debug!(client = %label, "control link closed");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn inst(id: &str, control: bool) -> Instance {
        let (tx, _rx) = mpsc::channel(1);
        Instance {
            instance_id: id.into(),
            inbox_tx: tx,
            display_name: None,
            boot_id: String::new(),
            hold: Duration::ZERO,
            gate: true,
            control,
            session: false,
        }
    }

    #[test]
    fn a_strict_join_never_falls_back() {
        let pool = [inst("a", true), inst("b", true), inst("old", false)];
        assert!(matches!(decide(&pool, Some("b"), true), Decision::Go(i) if i.instance_id == "b"));
        // Its daemon is away: it waits for it rather than take another.
        assert!(matches!(
            decide(&pool, Some("gone"), true),
            Decision::Park(STRICT_AWAY)
        ));
        assert!(matches!(
            decide(&[], Some("gone"), true),
            Decision::Park(STRICT_AWAY)
        ));
        // Its daemon is there but too old for seatless rooms: refused, not moved.
        assert!(matches!(decide(&pool, Some("old"), true), Decision::Refuse));
        // Not strict: the same misses fall back, or wait only for an empty pool.
        assert!(
            matches!(decide(&pool, Some("gone"), false), Decision::Go(i) if i.instance_id == "a")
        );
        assert!(matches!(decide(&[], None, false), Decision::Park(r) if r.contains("no computer")));
        // Strict with no instance named is an ordinary join.
        assert!(matches!(decide(&pool, None, true), Decision::Go(i) if i.instance_id == "a"));
    }

    #[test]
    fn a_control_link_goes_only_to_a_daemon_that_serves_one() {
        let pool = [inst("a", false), inst("b", true), inst("c", true)];
        assert_eq!(pick(&pool, None).unwrap().instance_id, "b");
        assert_eq!(pick(&pool, Some("c")).unwrap().instance_id, "c");
        // The device's own daemon is too old: another that serves control links will do.
        assert_eq!(pick(&pool, Some("a")).unwrap().instance_id, "b");
        assert!(pick(&[inst("a", false)], None).is_none());
    }

    #[test]
    fn a_frame_is_carried_whole() {
        let w = wrap("r", r#"{"type":"sealed","n":3,"c":"AA"}"#).unwrap();
        match serde_json::from_str::<WireMsg>(&w).unwrap() {
            WireMsg::Ctl { room_id, text } => {
                assert_eq!(room_id, "r");
                assert_eq!(text, r#"{"type":"sealed","n":3,"c":"AA"}"#);
            }
            other => panic!("{other:?}"),
        }
    }
}
