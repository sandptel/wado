//! One participant's peer connection: the shared video and audio tracks bound into it (one
//! encode, every viewer — webrtc-rs 0.17 writes each packet to every binding of a track), its
//! input channels tagged with its origin, its presence channel, and its state reported to the
//! [`Hub`](super::Hub), which folds everyone's into what the compositor sees.
//!
//! The same for the seat's viewer and a joined room's: moved here from `relay_client`, where it
//! served the seat alone.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tracing::{info, warn};
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtcp::receiver_report::ReceiverReport;
use webrtc::track::track_local::TrackLocal;

use wado_compositor::InputEvent;
use wado_protocol::{FAST_CHANNEL, INPUT_CHANNEL, MOTION_CHANNEL, multi::PRESENCE_CHANNEL};

use super::Hub;
use super::trickle::Trickle;
use crate::session::membership::Origin;

/// Where a participant's current peer connection lives, and the generation that lets a stale
/// one's teardown be ignored once it has been replaced.
#[derive(Clone, Default)]
pub struct Slot {
    pub pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
    pub generation: Arc<AtomicU64>,
}

impl Slot {
    pub fn current(&self) -> Option<Arc<RTCPeerConnection>> {
        self.pc.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Close and forget it. Closed, not dropped: webrtc-rs frees its ICE sockets only on
    /// `close()` (I14).
    pub fn close(&self) {
        if let Some(pc) = self.pc.lock().unwrap_or_else(|e| e.into_inner()).take() {
            tokio::spawn(async move {
                let _ = pc.close().await;
            });
        }
    }
}

fn offer_has(sdp: &str, kind: &str) -> bool {
    sdp.lines().any(|l| l.starts_with(&format!("m={kind}")))
}

/// Summarise the ICE candidate types present in an SDP — "host", "srflx", "relay".
pub fn candidate_types(sdp: &str) -> String {
    let mut types: Vec<&str> = sdp
        .lines()
        .filter(|l| l.starts_with("a=candidate:"))
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            // a=candidate:<foundation> <component> <proto> <priority> <ip> <port> typ <type>
            parts.position(|w| w == "typ").and_then(|_| parts.next())
        })
        .collect();
    types.sort_unstable();
    types.dedup();
    if types.is_empty() {
        "none".to_string()
    } else {
        types.join(",")
    }
}

/// Answer one participant's offer: a new peer connection in `slot`, replacing (and closing) the
/// one before. Returns the answer's JSON, and the [`Trickle`] to open once it has been sent —
/// candidates gathered after it then follow on `out` (`super::trickle`).
pub async fn answer(
    hub: &Arc<Hub>,
    origin: Origin,
    offer_json: &str,
    slot: &Slot,
    peer: String,
    out: Option<tokio::sync::mpsc::Sender<String>>,
) -> crate::Result<(String, Trickle)> {
    let offer: RTCSessionDescription = serde_json::from_str(offer_json)
        .map_err(|e| crate::WadoError::Other(format!("bad SDP: {e}")))?;
    let offered_at = std::time::Instant::now();
    let remote_types = candidate_types(&offer.sdp);

    // The offer carries every candidate the browser gathered (non-trickle). Their types are
    // the whole diagnosis when media never flows: host-only means STUN was blocked and no
    // route past NAT was ever found, srflx present means the path failed somewhere later.
    info!(
        peer = %peer,
        origin,
        "relay client: offer received — {} candidates ({})",
        offer.sdp.matches("a=candidate:").count(),
        remote_types
    );

    let pc = Arc::new(
        hub.api
            .new_peer_connection(RTCConfiguration {
                ice_servers: crate::ice::servers(),
                ..Default::default()
            })
            .await?,
    );

    // Each track only when the offer asked for it: an older client offers video alone, and a
    // shell-only "play on this phone" offers audio alone (no session, nothing to encode). The
    // tracks are the session's one pair: every participant's connection binds the same ones.
    let has_audio = offer_has(&offer.sdp, "audio");
    let has_video = offer_has(&offer.sdp, "video");
    let rtp_sender = if has_video {
        Some(
            pc.add_track(Arc::clone(&hub.track) as Arc<dyn TrackLocal + Send + Sync>)
                .await?,
        )
    } else {
        None
    };
    if has_audio {
        pc.add_track(Arc::clone(&hub.audio_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await?;
    }

    // Input data channels: forward JSON InputEvents to the compositor, tagged with this
    // participant's origin. Both of them — the reliable INPUT_CHANNEL and the unreliable
    // MOTION_CHANNEL for pointer/touch moves (invariant #1) — plus the fast copies. The presence
    // channel is the daemon's to write on, never read.
    {
        let hub_dc = Arc::clone(hub);
        // Shared by the input and fast channels of this peer connection: redundant copies are
        // released once, in order (`crate::input_order`).
        let order = Arc::new(Mutex::new(crate::input_order::InputOrder::default()));
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let hub = Arc::clone(&hub_dc);
            let order = Arc::clone(&order);
            Box::pin(async move {
                let label = dc.label().to_string();
                if label == PRESENCE_CHANNEL {
                    hub.set_presence_channel(origin, Arc::clone(&dc));
                    return;
                }
                if label != INPUT_CHANNEL && label != MOTION_CHANNEL && label != FAST_CHANNEL {
                    return;
                }
                dc.on_open(Box::new(move || {
                    Box::pin(async move { info!("relay client: {label} data channel open") })
                }));
                let dc_echo = Arc::clone(&dc);
                dc.on_message(Box::new(move |msg: DataChannelMessage| {
                    let hub = Arc::clone(&hub);
                    let dc_echo = Arc::clone(&dc_echo);
                    let order = Arc::clone(&order);
                    Box::pin(async move {
                        // A sequenced copy ("Redundant input"): released once, in order.
                        if let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(&msg.data) {
                            if let Some(q) = v.get("q").and_then(|q| q.as_u64()) {
                                if let Some(o) = v.as_object_mut() {
                                    o.remove("q");
                                }
                                let ready = {
                                    let mut o = order.lock().unwrap_or_else(|e| e.into_inner());
                                    let r = o.arrive(q, v);
                                    // Every 500 released: how often a copy was a duplicate, a move
                                    // stale, or a strict event held for a gap.
                                    if !r.is_empty()
                                        && o.stats.released / 500
                                            != (o.stats.released - r.len() as u64) / 500
                                    {
                                        let st = o.stats;
                                        info!(
                                            released = st.released,
                                            duplicates = st.duplicates,
                                            stale_moves = st.stale_moves,
                                            held = st.held,
                                            "redundant input"
                                        );
                                    }
                                    r
                                };
                                for ev in ready {
                                    match serde_json::from_value::<InputEvent>(ev) {
                                        Ok(ev) => hub.input(origin, ev),
                                        Err(e) => warn!("relay client: bad input event: {e}"),
                                    }
                                }
                                return;
                            }
                        }
                        match serde_json::from_slice::<InputEvent>(&msg.data) {
                            // The input round-trip probe: answered straight back on the channel it
                            // came in on. Direct mode always did; relay mode did not, so the
                            // phone's "input" figure could never be measured (2026-10-03).
                            Ok(InputEvent::Ping { seq }) => {
                                let _ = dc_echo
                                    .send_text(format!("{{\"t\":\"pong\",\"seq\":{seq}}}"))
                                    .await;
                            }
                            Ok(ev) => hub.input(origin, ev),
                            Err(e) => warn!("relay client: bad input event: {e}"),
                        }
                    })
                }));
            })
        }));
    }

    // Generation guard: only the current connection's teardown counts.
    let my_gen = slot.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let previous = slot
        .pc
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Arc::clone(&pc));
    hub.set_pc(origin, Arc::clone(&pc));

    // Dropping an `RTCPeerConnection` does NOT free it: webrtc-rs holds the ICE agent, its
    // gathering tasks and its UDP sockets behind internal `Arc`s, and only `close()` tears them
    // down. Replacing the handle without closing leaks a socket set per re-offer.
    //
    // Observed 2026-09-12: after a few hours of reconnect churn the daemon began answering with
    // `2 candidates (host)` and then `0 candidates (none)` — no srflx, eventually not even a host
    // candidate — and no client could connect until it was restarted.
    //
    // Spawned rather than awaited: `close()` waits on the agent's own tasks, and this is the
    // negotiation path — the new answer must not queue behind the old connection's shutdown.
    if let Some(old) = previous {
        tokio::spawn(async move {
            if let Err(e) = old.close().await {
                warn!("relay client: closing the previous peer connection: {e}");
            }
        });
    }

    // RTCP read loop: PLI/FIR → a keyframe (coalesced across participants).
    if let Some(rtp_sender) = rtp_sender {
        let hub = Arc::clone(hub);
        tokio::spawn(async move {
            while let Ok((packets, _)) = rtp_sender.read_rtcp().await {
                for p in packets {
                    let a = p.as_any();
                    if a.downcast_ref::<PictureLossIndication>().is_some()
                        || a.downcast_ref::<FullIntraRequest>().is_some()
                    {
                        hub.want_keyframe();
                    }
                    // Loss on the video path: downloads back off (`files::pace`).
                    if let Some(rr) = a.downcast_ref::<ReceiverReport>() {
                        for r in &rr.reports {
                            crate::files::pace::loss(r.fraction_lost);
                        }
                    }
                }
            }
        });
    }

    // Connection state change.
    {
        let hub = Arc::clone(hub);
        let generation = Arc::clone(&slot.generation);
        let pc_peer = peer.clone();
        pc.on_peer_connection_state_change(Box::new(move |state| {
            match state {
                RTCPeerConnectionState::Connected => {
                    info!(peer = %pc_peer, origin, took_ms = offered_at.elapsed().as_millis() as u64,
                          "relay client: viewer connected via WebRTC");
                    hub.pc_state(origin, true, has_audio, has_video);
                }
                // NOT a teardown. A peer connection dying is a *transport* event — a cell
                // handoff, a tunnel change, a few seconds in a lift — and killing the session
                // here meant every one of those cost the viewer their windows, their
                // applications and a cold Chrome launch. Measured 2026-09-12.
                //
                // `viewer_watchdog` is the only teardown of the session; a joined participant
                // whose connection stays down is dropped from it by the hub's own watchdog.
                RTCPeerConnectionState::Failed
                | RTCPeerConnectionState::Closed
                | RTCPeerConnectionState::Disconnected => {
                    if generation.load(Ordering::SeqCst) == my_gen {
                        hub.pc_state(origin, false, has_audio, has_video);
                        info!(
                            peer = %pc_peer,
                            origin,
                            "relay client: peer connection {state:?} — session kept, waiting for \
                             a re-offer (the watchdog stops it if no viewer comes back)"
                        );
                    }
                }
                other => info!(?other, "relay client: peer connection state"),
            }
            Box::pin(async {})
        }));
    }

    // ICE-level transitions. The peer-connection state hides where a failure happened;
    // this is the one that distinguishes "never got a reply" (stuck Checking) from
    // "candidates exhausted" (Failed).
    let ice_peer = peer.clone();
    let ice_remote = remote_types.clone();
    let local_types = Arc::new(Mutex::new("<not answered yet>".to_string()));
    let ice_local = Arc::clone(&local_types);
    pc.on_ice_connection_state_change(Box::new(move |state| {
        let (peer, remote) = (ice_peer.clone(), ice_remote.clone());
        let local = ice_local.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let ms = offered_at.elapsed().as_millis() as u64;
        match state {
            RTCIceConnectionState::Failed | RTCIceConnectionState::Disconnected => warn!(
                peer = %peer, ms, local = %local, remote = %remote,
                "relay client: ICE {state:?} after {ms}ms — we offered ({local}), the device \
                 offered ({remote}), and neither has a `relay` candidate because wado runs STUN \
                 only. ⇒ ASK ABOUT A VPN FIRST, on either end: it adds its tunnel interface to \
                 the gather and makes the NAT symmetric, which fails exactly like this. \
                 Confirmed 2026-09-19 — Zscaler on the client, 15 candidates and never \
                 connected; switched off, 9 candidates and connected in under a second. A \
                 symmetric carrier NAT does the same thing. Check the startup NAT verdict above."
            ),
            _ => info!(peer = %peer, ms, ?state, "relay client: ICE connection state"),
        }
        Box::pin(async {})
    }));

    let trickle = Trickle::watch(&pc, out);
    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    // Not until gathering *completes* (a dead STUN server makes that seconds): until the answer
    // has what a connection needs, the rest trickling after it (`super::trickle`). Bounded by
    // `crate::ice::GATHER_WAIT` all the same.
    if trickle.enough(&mut gather, crate::ice::GATHER_WAIT).await {
        warn!(
            "ICE gathering found nothing usable in {:?} — answering with what we have. A STUN \
             server is not replying; webrtc-ice gives each one a fixed 5s before giving up.",
            crate::ice::GATHER_WAIT
        );
    }

    let local = pc
        .local_description()
        .await
        .ok_or_else(|| crate::WadoError::Other("no local description after ICE gather".into()))?;
    let answer_json = serde_json::to_string(&local)?;

    let local_kinds = candidate_types(&local.sdp);
    *local_types.lock().unwrap_or_else(|e| e.into_inner()) = local_kinds.clone();
    info!(
        peer = %peer,
        origin,
        gather_ms = offered_at.elapsed().as_millis() as u64,
        "relay client: answer sent — {} candidates ({})",
        local.sdp.matches("a=candidate:").count(),
        local_kinds
    );
    // Said loudly because the alternative is watching ICE fail and guessing. Host-only is not
    // a weaker connection, it is one that cannot be made from outside this LAN.
    if !crate::ice::has_reflexive(&local.sdp) {
        warn!(
            "no server-reflexive candidate — every STUN server timed out, so this answer only \
             works on the local network. A client elsewhere hangs on 'starting session' while \
             ICE goes checking, disconnected, failed."
        );
    }
    Ok((answer_json, trickle))
}
