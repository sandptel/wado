//! The session's sound, streamed: its audio sink's monitor → Opus → a WebRTC audio track.
//!
//! Captures the daemon's "This phone" sink (`crate::host::phone_sink`) while a viewer is
//! listening: a thread reads PCM from it, encodes 10 ms Opus frames and hands them to the track. Audio
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

/// Stream the phone sink while someone is listening — a peer connection that asked for audio
/// is up — and stop when nobody is, so an idle daemon encodes nothing.
pub async fn run(
    sink: String,
    mut listening: watch::Receiver<bool>,
    mut low: watch::Receiver<bool>,
    track: Arc<TrackLocalStaticSample>,
) {
    loop {
        let on = *listening.borrow_and_update();
        let frame_ms = if *low.borrow_and_update() { 5 } else { 10 };
        // A stop flag the capture thread checks once per frame; dropping the sender stops it.
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        if on {
            let (tx, mut rx) = mpsc::channel::<Bytes>(50);
            let name = sink.clone();
            std::thread::Builder::new()
                .name("wado-audio".into())
                .spawn(move || pump(&name, frame_ms, tx, stop_rx))
                .ok();
            let track = Arc::clone(&track);
            tokio::spawn(async move {
                let mut spacing = Spacing::default();
                while let Some(data) = rx.recv().await {
                    spacing.tick(frame_ms);
                    let _ = track
                        .write_sample(&Sample {
                            data,
                            duration: Duration::from_millis(frame_ms.into()),
                            ..Default::default()
                        })
                        .await;
                }
            });
        }
        // Either changing restarts the pump: listening on/off, or a new frame size.
        let changed = tokio::select! {
            r = listening.changed() => r,
            r = low.changed() => r,
        };
        if changed.is_err() {
            break;
        }
        drop(stop_tx);
    }
}

/// How evenly audio packets leave, logged every 5 s. Chrome sizes its audio jitter buffer to the
/// unevenness of arrival, and video is held level with audio (A/V sync is mandatory), so a
/// bursty sender costs the *picture* latency. Measured 2026-10-03: audio buffer 183 ms and video
/// 188 ms on an 11 ms round trip — the question this answers is whether the bursts start here.
#[derive(Default)]
struct Spacing {
    last: Option<std::time::Instant>,
    gaps_ms: Vec<f64>,
    since: Option<std::time::Instant>,
}

impl Spacing {
    fn tick(&mut self, frame_ms: u32) {
        let now = std::time::Instant::now();
        if let Some(prev) = self.last.replace(now) {
            self.gaps_ms
                .push(now.duration_since(prev).as_secs_f64() * 1000.0);
        }
        let since = *self.since.get_or_insert(now);
        if now.duration_since(since) >= Duration::from_secs(5) && !self.gaps_ms.is_empty() {
            let g = &mut self.gaps_ms;
            g.sort_by(f64::total_cmp);
            let at = |q: f64| g[((g.len() - 1) as f64 * q) as usize];
            // A "burst" packet left within 1 ms of the one before: it was waiting, not paced.
            let burst = g.iter().filter(|&&x| x < 1.0).count();
            tracing::info!(
                frame_ms,
                packets = g.len() + 1,
                gap_p50_ms = format!("{:.1}", at(0.5)),
                gap_p90_ms = format!("{:.1}", at(0.9)),
                gap_max_ms = format!("{:.1}", at(1.0)),
                burst_pct = format!("{:.0}", 100.0 * burst as f64 / g.len() as f64),
                "audio send spacing"
            );
            g.clear();
            self.since = Some(now);
        }
    }
}

fn pump(sink: &str, frame_ms: u32, tx: mpsc::Sender<Bytes>, stop: std::sync::mpsc::Receiver<()>) {
    let kbps = wado_config::live::current()
        .session
        .audio_bitrate
        .clamp(16, 256);
    let (mut cap, mut enc) = match (
        capture::Capture::start(sink, frame_ms),
        opus::Opus::new(kbps, frame_ms),
    ) {
        (Ok(c), Ok(e)) => (c, e),
        (Err(e), _) => return tracing::warn!("no session audio: pw-record did not start ({e})"),
        (_, Err(e)) => return tracing::warn!("no session audio: Opus encoder did not open ({e})"),
    };
    tracing::info!(sink, kbps, frame_ms, "streaming session audio");
    let mut pcm = vec![0u8; opus::frame_bytes(frame_ms)];
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
