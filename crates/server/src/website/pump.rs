//! The direct-mode frame pump — one job: move encoded frames from the compositor onto the
//! shared WebRTC video track, measuring how long each waited to be picked up.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use tokio::sync::mpsc;
use tracing::warn;
use wado_compositor::FrameMsg;
use webrtc::media::Sample;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

/// Frame pump: encoded frames → write_sample. Harmless no-op when no viewer.
pub(super) fn spawn(
    mut frame_rx: mpsc::Receiver<FrameMsg>,
    track: Arc<TrackLocalStaticSample>,
    queue_us: Arc<AtomicU64>,
) {
    tokio::spawn(async move {
        while let Some(frame) = frame_rx.recv().await {
            // Stamped by the compositor at hand-off, so this is pure waiting time.
            let waited = frame.queued_at.elapsed().as_micros() as u64;
            // EWMA, 1/8 weight on the newest sample.
            let prev = queue_us.load(Ordering::Relaxed);
            queue_us.store((prev * 7 + waited) / 8, Ordering::Relaxed);

            let sample = Sample {
                data: Bytes::from(frame.data),
                duration: frame.duration,
                ..Default::default()
            };
            if let Err(e) = track.write_sample(&sample).await {
                warn!("write_sample error: {e}");
            }
        }
    });
}
