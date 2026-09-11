//! A [`FrameSink`] that forwards encoded frames to the `website` control plane's
//! WebRTC frame pump over a bounded, drop-on-full channel.
//!
//! The render loop (sync, calloop thread) calls [`FrameSink::send`], which does a
//! non-blocking `try_send`; the tokio pump task on the other end owns the matching
//! receiver and calls `write_sample` on the shared video track. This keeps the
//! render tick from ever blocking on the network. Dropped frames (channel full /
//! no viewer) are recovered by the next IDR.
//!
//! Each frame carries the **real elapsed time since the previous frame we handed over**,
//! so the pump can set RTP timestamps without knowing the session's fps. Real elapsed
//! time, not a nominal 1/fps: the RTP clock has to track wall clock, because any gap —
//! a dropped frame, a long tick, a downgrade rebuild — otherwise advances wall clock
//! while leaving the RTP clock behind. The receiver compensates for that mismatch by
//! growing its playout buffer, i.e. by adding latency that looks like a network problem
//! but is manufactured here.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::FrameSink;

/// One encoded access unit on its way to the WebRTC pump.
///
/// A struct rather than a tuple because the third field is easy to misread positionally,
/// and the queue measurement below only means anything if it is stamped at the right
/// moment.
#[derive(Debug)]
pub struct FrameMsg {
    /// The encoded access unit (Annex-B).
    pub data: Vec<u8>,
    /// Real time elapsed since the previous frame handed over, for RTP pacing.
    pub duration: Duration,
    /// When the compositor handed this frame over. The pump subtracts this on receipt to
    /// report how long the frame waited — the one pipeline leg the compositor cannot see,
    /// because it happens after it lets go.
    pub queued_at: Instant,
}

/// Report dropped frames at most once per this many drops, so a persistently full
/// channel warns without flooding the log (and the SSE log panel) every frame.
const DROP_REPORT_EVERY: u64 = 60;

pub struct ChannelSink {
    tx: mpsc::Sender<FrameMsg>,
    /// Nominal 1/fps — used only for the very first frame, where there is no previous
    /// hand-off to measure against.
    frame_dur: Duration,
    /// When we last handed a frame to the pump, for the real elapsed-time stamp.
    last_sent: Option<Instant>,
    /// Frames the pump could not accept. Counted rather than swallowed because a
    /// standing drop rate means the render loop is outrunning the network, and the
    /// viewer is seeing stale frames.
    dropped: u64,
}

impl ChannelSink {
    pub fn new(tx: mpsc::Sender<FrameMsg>, frame_dur: Duration) -> Self {
        Self {
            tx,
            frame_dur,
            last_sent: None,
            dropped: 0,
        }
    }
}

impl FrameSink for ChannelSink {
    fn send(&mut self, nal_data: &[u8]) {
        let now = Instant::now();
        // Measure from the last frame we actually handed over, so time spent on frames the
        // pump refused is still accounted for in the next successful sample's duration.
        let dur = match self.last_sent {
            Some(prev) => now.saturating_duration_since(prev),
            None => self.frame_dur,
        };
        let msg = FrameMsg {
            data: nal_data.to_vec(),
            duration: dur,
            queued_at: now,
        };
        if self.tx.try_send(msg).is_err() {
            self.dropped += 1;
            if self.dropped % DROP_REPORT_EVERY == 0 {
                tracing::warn!(
                    dropped = self.dropped,
                    "encoded frames dropped — pump full; viewer is seeing stale frames"
                );
            }
            // Deliberately do NOT advance `last_sent` here: the dropped frame's share of
            // wall clock rolls into the next accepted sample, keeping the RTP clock on
            // wall clock across the gap.
        } else {
            self.last_sent = Some(now);
        }
    }
}
