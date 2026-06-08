//! SSE broadcast task for compositor session events (one job: encoder-changed).
//!
//! The compositor pushes a [`wado_compositor::SessionEvent`] when the pipeline tier
//! degrades at runtime. This module drains that channel, serialises the payload, and
//! broadcasts a pre-formatted SSE frame to all active `/events` subscribers so they
//! update the pipeline badge live — without waiting for the next page load.

use tokio::sync::{broadcast, mpsc};
use wado_compositor::SessionEvent;

/// Broadcast capacity for encoder SSE frames. Downgrades happen at most once per tier
/// per session, so a small buffer is more than enough.
pub const SSE_CAPACITY: usize = 16;

/// Spawns a background task that drains `event_rx` and broadcasts pre-formatted SSE
/// frames to all `/events` subscribers. Two named event types are emitted:
/// - `event: encoder` — encoder tier changed at runtime (pipeline badge update).
/// - `event: session` — session ended abnormally (render panic or all tiers exhausted).
///
/// Call this once after `tokio::runtime` is live, before the listener starts.
pub fn spawn(mut event_rx: mpsc::Receiver<SessionEvent>, sse_tx: broadcast::Sender<String>) {
    tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            let frame = match ev {
                SessionEvent::EncoderChanged(report) => {
                    match serde_json::to_string(&report) {
                        Ok(json) => format!("event: encoder\ndata: {json}\n\n"),
                        Err(e) => {
                            tracing::warn!("failed to serialize EncoderReport for SSE: {e}");
                            continue;
                        }
                    }
                }
                SessionEvent::SessionEnded { reason } => {
                    let json = serde_json::json!({ "reason": reason }).to_string();
                    format!("event: session\ndata: {json}\n\n")
                }
            };
            // Pre-formatted SSE frame; serve_sse writes it verbatim.
            let _ = sse_tx.send(frame);
        }
    });
}
