//! Per-stage render-pipeline timing — one job: measure how long each step of a frame
//! actually takes, and publish a rolling average the server can serve to the client.
//!
//! Distinct from [`crate::pacing`], which watches only the *interval* between ticks to
//! detect a starved event loop. This module attributes time **within** a tick: capture,
//! encode, and the wait before the WebRTC pump accepts the result. That attribution is
//! what turns "it feels laggy" into "the encode is fine, the queue is the problem".
//!
//! Published through a [`tokio::sync::watch`] channel: latest-value-wins with no backlog,
//! which is exactly right for telemetry — a reader that falls behind wants the newest
//! numbers, never a queue of stale ones. Dropping the receiver is not an error, it just
//! means nobody is looking.

use std::time::{Duration, Instant};

use tokio::sync::watch;
use wado_protocol::StageTimings;

/// Publish a new average this often. Matches the client's poll interval closely enough
/// that it always has fresh numbers without the compositor doing needless work.
const PUBLISH_INTERVAL: Duration = Duration::from_millis(500);

/// Accumulates per-frame stage durations and publishes their mean on an interval.
pub struct StageTimer {
    tx: watch::Sender<StageTimings>,
    last_publish: Instant,
    /// Previous tick start, for the achieved frame interval.
    last_tick: Option<Instant>,

    frames: u32,
    capture: Duration,
    encode: Duration,
    queue: Duration,
    tick: Duration,
    dropped: u64,
}

impl StageTimer {
    pub fn new() -> (Self, watch::Receiver<StageTimings>) {
        let (tx, rx) = watch::channel(StageTimings::default());
        let timer = Self {
            tx,
            last_publish: Instant::now(),
            last_tick: None,
            frames: 0,
            capture: Duration::ZERO,
            encode: Duration::ZERO,
            queue: Duration::ZERO,
            tick: Duration::ZERO,
            dropped: 0,
        };
        (timer, rx)
    }

    /// Record one completed frame. `queue` is the time the encoded frame waited before the
    /// pump took it; pass [`Duration::ZERO`] when there was no frame to hand over.
    pub fn frame(&mut self, start: Instant, capture: Duration, encode: Duration, queue: Duration) {
        if let Some(prev) = self.last_tick {
            self.tick += start.saturating_duration_since(prev);
        }
        self.last_tick = Some(start);

        self.frames += 1;
        self.capture += capture;
        self.encode += encode;
        self.queue += queue;

        if self.last_publish.elapsed() >= PUBLISH_INTERVAL {
            self.publish();
        }
    }

    /// Note that the pump refused a frame (counted cumulatively, not averaged).
    pub fn dropped(&mut self) {
        self.dropped += 1;
    }

    fn publish(&mut self) {
        let n = self.frames.max(1);
        let ms = |d: Duration| d.as_secs_f64() * 1e3 / n as f64;
        let tick_ms = ms(self.tick);

        // A watch send only fails when every receiver is gone — nobody is reading the
        // telemetry, which is fine and not worth logging per window.
        let _ = self.tx.send(StageTimings {
            capture_ms: ms(self.capture),
            encode_ms: ms(self.encode),
            queue_ms: ms(self.queue),
            tick_ms,
            fps: if tick_ms > 0.0 { 1e3 / tick_ms } else { 0.0 },
            dropped: self.dropped,
        });

        self.last_publish = Instant::now();
        self.frames = 0;
        self.capture = Duration::ZERO;
        self.encode = Duration::ZERO;
        self.queue = Duration::ZERO;
        self.tick = Duration::ZERO;
    }
}
