//! The low-latency media path ("Low-latency pipeline (WebCodecs)", Display & stream): video and
//! audio over an unreliable, unordered WebRTC data channel instead of RTP, so the phone decodes
//! them itself (WebCodecs) and keeps sync with a buffer it sizes — Chrome's audio jitter buffer
//! grew to 450–600 ms on a jittery link and held the picture to it (Decision Log 2026-10-03).
//!
//! Opt-in per session (`SessionConfig::webcodecs`). While it is off, or no media channel is open,
//! everything goes to the RTP tracks exactly as before — that path is the fallback.

pub mod wire;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bytes::Bytes;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_state::RTCDataChannelState;

/// Past this much unsent data, a frame is dropped rather than queued: a queue is latency, and
/// the receiver asks for a keyframe when it sees the gap.
const MAX_BUFFERED: usize = 256 * 1024;

pub struct Hub {
    dc: Mutex<Option<Arc<RTCDataChannel>>>,
    wanted: AtomicBool,
    /// "Redundant audio": each packet goes again with the next two.
    audio_redundancy: AtomicBool,
    recent_audio: Mutex<std::collections::VecDeque<(u32, u64, Vec<u8>)>>,
    video_seq: AtomicU32,
    audio_seq: AtomicU32,
    epoch: Instant,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            dc: Mutex::new(None),
            wanted: AtomicBool::new(false),
            audio_redundancy: AtomicBool::new(false),
            recent_audio: Mutex::new(std::collections::VecDeque::new()),
            video_seq: AtomicU32::new(0),
            audio_seq: AtomicU32::new(0),
            epoch: Instant::now(),
        }
    }
}

impl Hub {
    /// The viewer's media channel, or `None` when it closed.
    pub fn set_channel(&self, dc: Option<Arc<RTCDataChannel>>) {
        *self.dc.lock().unwrap_or_else(|e| e.into_inner()) = dc;
    }

    /// The session config asked for (or stopped asking for) this path.
    pub fn set_wanted(&self, on: bool) {
        self.wanted.store(on, Ordering::SeqCst);
    }

    pub fn set_audio_redundancy(&self, on: bool) {
        self.audio_redundancy.store(on, Ordering::SeqCst);
    }

    /// Media goes here instead of RTP: asked for, and a channel is open to carry it.
    pub fn active(&self) -> bool {
        self.wanted.load(Ordering::SeqCst) && self.channel().is_some()
    }

    fn channel(&self) -> Option<Arc<RTCDataChannel>> {
        self.dc
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .filter(|d| d.ready_state() == RTCDataChannelState::Open)
    }

    /// Microseconds on the one clock audio and video share.
    pub fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// The clock reading for an `Instant` taken elsewhere (a frame's encode time).
    pub fn at_us(&self, t: Instant) -> u64 {
        t.saturating_duration_since(self.epoch).as_micros() as u64
    }

    pub async fn send_video(&self, data: &[u8], key: bool, ts_us: u64) -> bool {
        let seq = self.video_seq.fetch_add(1, Ordering::Relaxed);
        self.send(wire::VIDEO, key, seq, ts_us, data).await
    }

    /// One Opus packet — and with "Redundant audio", the previous two again under their own
    /// seq and timestamp: each packet arrives up to three times, 10 ms apart, and the receiver
    /// keeps the first. A delay spike that holds one packet back is covered by its copy, so the
    /// buffer need not grow for it (and video waits on that buffer).
    pub async fn send_audio(&self, data: &[u8], ts_us: u64) -> bool {
        let seq = self.audio_seq.fetch_add(1, Ordering::Relaxed);
        let sent = self.send(wire::AUDIO, false, seq, ts_us, data).await;
        if self.audio_redundancy.load(Ordering::SeqCst) {
            let again: Vec<_> = {
                let mut r = self.recent_audio.lock().unwrap_or_else(|e| e.into_inner());
                let old = r.iter().cloned().collect();
                r.push_back((seq, ts_us, data.to_vec()));
                while r.len() > 2 {
                    r.pop_front();
                }
                old
            };
            for (s, t, d) in again {
                self.send(wire::AUDIO, false, s, t, &d).await;
            }
        }
        sent
    }

    /// False when the frame was not sent (no channel, or too much already queued).
    async fn send(&self, kind: u8, key: bool, seq: u32, ts_us: u64, data: &[u8]) -> bool {
        let Some(dc) = self.channel() else {
            return false;
        };
        // Audio is never the one dropped for queue depth: it is small, and it is the clock.
        if kind == wire::VIDEO && dc.buffered_amount().await > MAX_BUFFERED {
            return false;
        }
        for c in wire::chunks(kind, key, seq, ts_us, data) {
            if dc.send(&Bytes::from(c)).await.is_err() {
                return false;
            }
        }
        true
    }
}

/// Shared between the frame pump, the audio pump and the relay client.
pub type SharedHub = Arc<Hub>;
