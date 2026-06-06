//! A [`FrameSink`] that forwards encoded frames to the `website` control plane's
//! WebRTC frame pump over a bounded, drop-on-full channel.
//!
//! The render loop (sync, calloop thread) calls [`FrameSink::send`], which does a
//! non-blocking `try_send`; the tokio pump task on the other end owns the matching
//! receiver and calls `write_sample` on the shared video track. This keeps the
//! render tick from ever blocking on the network. Dropped frames (channel full /
//! no viewer) are recovered by the next IDR.
//!
//! Each frame carries its sample duration (1/fps) so the pump can set correct RTP
//! timestamps without knowing the session's fps.

use std::time::Duration;

use tokio::sync::mpsc;

use super::FrameSink;

/// One encoded access unit plus its presentation duration.
pub type FrameMsg = (Vec<u8>, Duration);

pub struct ChannelSink {
    tx: mpsc::Sender<FrameMsg>,
    frame_dur: Duration,
}

impl ChannelSink {
    pub fn new(tx: mpsc::Sender<FrameMsg>, frame_dur: Duration) -> Self {
        Self { tx, frame_dur }
    }
}

impl FrameSink for ChannelSink {
    fn send(&mut self, nal_data: &[u8]) {
        let _ = self.tx.try_send((nal_data.to_vec(), self.frame_dur));
    }
}
