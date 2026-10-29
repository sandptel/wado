//! The trackpad's direct link: its own peer connection, opened by a `pad_offer` that came through
//! the sealed envelope, separate from the session's and the files' so touch never shares an SCTP
//! association with video or a transfer (invariant #1). Two channels: `pad-touch` (unordered,
//! no retransmits — a frame holds every finger, so the newest is all that matters) and
//! `pad-keys` (ordered, reliable — a lost key or lift cannot be recovered).

use std::sync::Arc;

use tracing::warn;
use webrtc::api::API;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use wado_protocol::pad::{KEYS_CHANNEL, PadMsg, TOUCH_CHANNEL};

use super::live::{Live, Peer};

/// Build the pad's peer connection for `offer`; the answer to send back.
pub async fn open(
    api: &API,
    offer: &str,
    live: Live,
    peer: Peer,
) -> crate::Result<(Arc<RTCPeerConnection>, String)> {
    let offer: RTCSessionDescription = serde_json::from_str(offer)
        .map_err(|e| crate::WadoError::Other(format!("bad pad SDP: {e}")))?;
    let pc = Arc::new(
        api.new_peer_connection(RTCConfiguration {
            ice_servers: crate::ice::servers(),
            ..Default::default()
        })
        .await?,
    );
    pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let (live, peer) = (live.clone(), peer.clone());
        Box::pin(async move {
            let label = dc.label().to_string();
            if label != TOUCH_CHANNEL && label != KEYS_CHANNEL {
                return;
            }
            if label == KEYS_CHANNEL {
                live.keys_opened(Arc::clone(&dc));
                let (closing, me) = (live.clone(), Arc::downgrade(&dc));
                dc.on_close(Box::new(move || {
                    if let Some(me) = me.upgrade() {
                        closing.keys_closed(&me);
                    }
                    Box::pin(async {})
                }));
            }
            dc.on_message(Box::new(move |m: DataChannelMessage| {
                match std::str::from_utf8(&m.data)
                    .ok()
                    .and_then(|s| serde_json::from_str::<PadMsg>(s).ok())
                {
                    Some(msg) => live.accept(msg, false, &peer),
                    None => warn!("pad: unreadable message on {label}"),
                }
                Box::pin(async {})
            }));
        })
    }));
    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    let _ = tokio::time::timeout(crate::ice::GATHER_WAIT, gather.recv()).await;
    let local = pc
        .local_description()
        .await
        .ok_or_else(|| crate::WadoError::Other("no local description".into()))?;
    Ok((pc, serde_json::to_string(&local)?))
}
