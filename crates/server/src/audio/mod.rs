//! The session's sound, streamed: its audio sink's monitor → Opus → a WebRTC audio track.
//!
//! Follows the compositor's sink (see `compositor::session_env::audio`): while there is one,
//! a thread reads PCM from it, encodes 10 ms Opus frames and hands them to the track. Audio
//! has its own track and its own RTP stream, so it never waits behind a video frame, and it
//! travels over SRTP/UDP like the video (invariant #2).
//!
//! - [`capture`] — PCM from PipeWire.
//! - [`opus`] — the encoder.

pub mod capture;
pub mod opus;

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use tokio::sync::{mpsc, watch};
use webrtc::{
    media::Sample, track::track_local::track_local_static_sample::TrackLocalStaticSample,
};

/// The audio track every peer connection carries, alongside the video one.
pub fn track() -> Arc<TrackLocalStaticSample> {
    Arc::new(TrackLocalStaticSample::new(
        webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability {
            mime_type: webrtc::api::media_engine::MIME_TYPE_OPUS.to_owned(),
            clock_rate: capture::RATE,
            channels: capture::CHANNELS as u16,
            ..Default::default()
        },
        "audio".to_owned(),
        "wado".to_owned(),
    ))
}

/// Follow the session's sink for the life of the daemon, streaming whatever it plays.
pub async fn run(mut sink: watch::Receiver<Option<String>>, track: Arc<TrackLocalStaticSample>) {
    loop {
        let now = sink.borrow_and_update().clone();
        // A stop flag the capture thread checks once per frame; dropping the sender stops it.
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        if let Some(name) = now {
            let (tx, mut rx) = mpsc::channel::<Bytes>(50);
            std::thread::Builder::new()
                .name("wado-audio".into())
                .spawn(move || pump(&name, tx, stop_rx))
                .ok();
            let track = Arc::clone(&track);
            tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    let _ = track
                        .write_sample(&Sample {
                            data,
                            duration: Duration::from_millis(10),
                            ..Default::default()
                        })
                        .await;
                }
            });
        }
        if sink.changed().await.is_err() {
            break;
        }
        drop(stop_tx);
    }
}

fn pump(sink: &str, tx: mpsc::Sender<Bytes>, stop: std::sync::mpsc::Receiver<()>) {
    let kbps = wado_config::live::current()
        .session
        .audio_bitrate
        .clamp(16, 256);
    let (mut cap, mut enc) = match (capture::Capture::start(sink), opus::Opus::new(kbps)) {
        (Ok(c), Ok(e)) => (c, e),
        (Err(e), _) => return tracing::warn!("no session audio: pw-record did not start ({e})"),
        (_, Err(e)) => return tracing::warn!("no session audio: Opus encoder did not open ({e})"),
    };
    tracing::info!(sink, kbps, "streaming session audio");
    let mut pcm = vec![0u8; opus::FRAME_BYTES];
    while matches!(stop.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty))
        && cap.read_frame(&mut pcm)
    {
        let sent = enc.encode(&pcm, |pkt| {
            // Never block the capture on a slow network: a frame that cannot be queued now is
            // a frame that would play late, which is worse than one that does not play.
            let _ = tx.try_send(Bytes::copy_from_slice(pkt));
        });
        if let Err(e) = sent {
            tracing::warn!("audio encode failed: {e}");
            break;
        }
    }
    tracing::info!("session audio stopped");
}
