//! WebRTC offer handling — one job: turn a viewer's SDP offer into a peer connection
//! carrying the shared video track and the input data channels, and answer it.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use tracing::{info, warn};
use wado_compositor::{CompositorCommand, InputEvent};
use wado_protocol::{INPUT_CHANNEL, MOTION_CHANNEL};
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::track::track_local::TrackLocal;

use super::ServerCtx;

/// Build a peer connection for one viewer, attach the shared track, wire RTCP
/// PLI→keyframe, and answer. A generation tag prevents a stale viewer's teardown
/// from killing a session started later.
pub(super) async fn handle_offer(ctx: &ServerCtx, offer_json: &str) -> crate::Result<String> {
    let offer: RTCSessionDescription = serde_json::from_str(offer_json)?;

    // STUN so ICE can discover server-reflexive candidates, enabling cross-NAT connections
    // when direct mode is port-forwarded. For pure localhost/LAN use, host candidates still
    // work without it. See `crate::ice` for why the list has three entries and not one.
    let pc = Arc::new(
        ctx.api
            .new_peer_connection(RTCConfiguration {
                ice_servers: crate::ice::servers(),
                ..Default::default()
            })
            .await?,
    );

    let rtp_sender = pc
        .add_track(Arc::clone(&ctx.track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // Remote input: the browser opens TWO data channels in its offer — INPUT_CHANNEL
    // (reliable+ordered: buttons, keys, scroll, touch, drag start/end) and MOTION_CHANNEL
    // (zero-retransmit: high-rate pointer/drag motion, latest-wins). Both carry
    // JSON InputEvents and both funnel into the same compositor input channel (never
    // behind video — invariant #1). Bad frames are dropped.
    //
    // The split exists because a high-polling-rate mouse saturates a reliable channel and
    // everything else then queues behind its backlog. See MOTION_CHANNEL's docs for why
    // each event type is safe on the channel it uses.
    {
        let input_tx = ctx.input_tx.clone();
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let input_tx = input_tx.clone();
            Box::pin(async move {
                let label = dc.label().to_string();
                if label != INPUT_CHANNEL && label != MOTION_CHANNEL {
                    info!("ignoring data channel {label:?} (not an input channel)");
                    return;
                }
                {
                    let label = label.clone();
                    dc.on_open(Box::new(move || {
                        let label = label.clone();
                        Box::pin(async move { info!("input data channel {label:?} open") })
                    }));
                }
                let input_tx = input_tx.clone();
                let dc_echo = Arc::clone(&dc);
                dc.on_message(Box::new(move |msg: DataChannelMessage| {
                    let input_tx = input_tx.clone();
                    let dc_echo = Arc::clone(&dc_echo);
                    Box::pin(async move {
                        match serde_json::from_slice::<InputEvent>(&msg.data) {
                            // Latency probe: bounce it straight back and do NOT forward it.
                            // Answering here measures the input path itself — if it went
                            // via the compositor the number would include a wait for the
                            // render loop, which is a different question.
                            Ok(InputEvent::Ping { seq }) => {
                                let pong = format!("{{\"t\":\"pong\",\"seq\":{seq}}}");
                                if let Err(e) = dc_echo.send_text(pong).await {
                                    tracing::debug!("pong send failed: {e}");
                                }
                            }
                            Ok(ev) => {
                                tracing::debug!(?ev, "input event");
                                if input_tx.send(ev).is_err() {
                                    warn!("compositor input channel closed — dropping input");
                                }
                            }
                            Err(e) => warn!("bad input event dropped: {e}"),
                        }
                    })
                }));
            })
        }));
    }

    // This viewer's generation; only this generation may auto-stop the session.
    let my_gen = ctx.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *ctx.active_pc.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(&pc));

    // RTCP read loop: the browser sends PLI/FIR when it needs a keyframe.
    let cmd_tx_rtcp = ctx.cmd_tx.clone();
    tokio::spawn(async move {
        loop {
            match rtp_sender.read_rtcp().await {
                Ok((packets, _)) => {
                    for p in packets {
                        let any = p.as_any();
                        if any.downcast_ref::<PictureLossIndication>().is_some()
                            || any.downcast_ref::<FullIntraRequest>().is_some()
                        {
                            let _ = cmd_tx_rtcp.send(CompositorCommand::ForceKeyframe);
                        }
                    }
                }
                Err(_) => break, // sender closed
            }
        }
    });

    let cmd_tx = ctx.cmd_tx.clone();
    let generation = Arc::clone(&ctx.generation);
    let active_pc = Arc::clone(&ctx.active_pc);
    pc.on_peer_connection_state_change(Box::new(move |state| {
        info!("viewer connection state: {state}");
        match state {
            // Force an IDR so the new viewer gets a picture immediately.
            RTCPeerConnectionState::Connected => {
                let _ = cmd_tx.send(CompositorCommand::ForceKeyframe);
            }
            // Only the current viewer tears the session down (generation guard).
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                if generation.load(Ordering::SeqCst) == my_gen {
                    info!("viewer gone — stopping session");
                    let _ = cmd_tx.send(CompositorCommand::Stop);
                    *active_pc.lock().unwrap_or_else(|e| e.into_inner()) = None;
                }
            }
            _ => {}
        }
        Box::pin(async {})
    }));

    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    // Bounded — see `crate::ice::GATHER_WAIT` and the twin in `relay_client.rs`.
    if tokio::time::timeout(crate::ice::GATHER_WAIT, gather_complete.recv())
        .await
        .is_err()
    {
        tracing::warn!(
            "ICE gathering still running after {:?} — answering with what we have",
            crate::ice::GATHER_WAIT
        );
    }

    let local = pc
        .local_description()
        .await
        .ok_or_else(|| crate::WadoError::Other("no local description after gathering".into()))?;
    Ok(serde_json::to_string(&local)?)
}
