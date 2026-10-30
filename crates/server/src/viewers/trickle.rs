//! When the answer has enough candidates to go, and the rest after it.
//!
//! Measured 2026-10-30 (`scripts/reconnect-time.mjs`): with this host's STUN slow to answer, the
//! answer waited the whole `crate::ice::GATHER_WAIT` (2 s) on every reconnect — most of the time
//! to picture, against a < 2 s target. The browser had the same wait and lost it the same way
//! (`js/ice_gather.js`). So the answer goes once a server-reflexive candidate is in (or, with
//! only host candidates, a little after the first), and every candidate is also sent after it as
//! `ice_candidate`, which the client adds to its peer connection — **held until the answer has
//! gone**, so none can arrive before the description it belongs to.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use wado_protocol::relay::RelayMsg;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_candidate_type::RTCIceCandidateType;
use webrtc::peer_connection::RTCPeerConnection;

/// After the first server-reflexive candidate, how long others get to join it.
const SETTLE: Duration = Duration::from_millis(60);
/// With only host candidates, how long after the first to keep waiting for a reflexive one.
const HOST_ONLY: Duration = Duration::from_millis(700);

/// Opened by [`Trickle::answered`]: candidates may follow the answer now.
pub struct Trickle {
    go: Arc<watch::Sender<bool>>,
    first_host: Arc<AtomicU64>,
    first_reflexive: Arc<AtomicU64>,
    start: Instant,
}

impl Trickle {
    /// Watch `pc`'s candidates. Each one goes to `out` as `ice_candidate` once the answer has
    /// (`None`: nothing to send them on — they ride the answer only).
    pub fn watch(pc: &RTCPeerConnection, out: Option<mpsc::Sender<String>>) -> Self {
        // Shared with the callback, so candidates found after the answer still see it opened.
        let go = Arc::new(watch::channel(false).0);
        let keep = Arc::clone(&go);
        let start = Instant::now();
        let first_host = Arc::new(AtomicU64::new(u64::MAX));
        let first_reflexive = Arc::new(AtomicU64::new(u64::MAX));
        let (h, r) = (Arc::clone(&first_host), Arc::clone(&first_reflexive));
        pc.on_ice_candidate(Box::new(move |c: Option<RTCIceCandidate>| {
            if let Some(c) = c {
                let at = start.elapsed().as_millis() as u64;
                let first = match c.typ {
                    RTCIceCandidateType::Host => &h,
                    _ => &r,
                };
                let _ = first.compare_exchange(u64::MAX, at, Ordering::SeqCst, Ordering::SeqCst);
                if let (Some(out), Ok(init)) = (out.clone(), c.to_json()) {
                    let mut go = keep.subscribe();
                    tokio::spawn(async move {
                        if go.wait_for(|v| *v).await.is_ok()
                            && let Ok(candidate) = serde_json::to_string(&init)
                            && let Ok(t) =
                                serde_json::to_string(&RelayMsg::IceCandidate { candidate })
                        {
                            let _ = out.send(t).await;
                        }
                    });
                }
            }
            Box::pin(async {})
        }));
        Self {
            go,
            first_host,
            first_reflexive,
            start,
        }
    }

    /// Wait until the answer has what a connection needs, gathering finishes, or `cap` passes.
    /// True when it had to give up at the cap.
    pub async fn enough(
        &self,
        gather: &mut tokio::sync::mpsc::Receiver<()>,
        cap: Duration,
    ) -> bool {
        let ms = |a: &AtomicU64| {
            let v = a.load(Ordering::SeqCst);
            (v != u64::MAX).then(|| Duration::from_millis(v))
        };
        loop {
            let now = self.start.elapsed();
            if ms(&self.first_reflexive).is_some_and(|t| now >= t + SETTLE)
                || ms(&self.first_host).is_some_and(|t| now >= t + HOST_ONLY)
            {
                return false;
            }
            if now >= cap {
                return true;
            }
            tokio::select! {
                _ = gather.recv() => return false,
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
    }

    /// The answer is on its way: candidates may follow it.
    pub fn answered(&self) {
        let _ = self.go.send(true);
    }
}
