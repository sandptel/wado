//! The link speed test (the user's decision, `2026-10-30`): before a device starts a session or
//! joins one, it pulls a burst of filler from this daemon ([`send`]) and reports what it measured
//! (`link_report`). [`verdict::judge`] then says whether that would work — and the daemon keeps
//! the answer, refusing the join or start it was about when it said `refuse`.
//!
//! The burst rides the device's own relay link (signalling, never video — invariant 2 is about
//! media), sealed like everything else, so it measures the path the computer's answers take.

pub mod headroom;
pub mod verdict;

use tokio::sync::mpsc;
use wado_protocol::relay::RelayMsg;

/// One chunk of filler. Sealed, so it is not compressible on the way.
const CHUNK: usize = 16 * 1024;
/// The most one test pulls.
pub const MAX_BYTES: u32 = 1024 * 1024;

/// Send `bytes` of filler down `out`, then `link_test_done`. Spawned: the burst must not hold up
/// the link's other messages for longer than it takes to queue.
pub fn send(out: mpsc::Sender<String>, bytes: u32) {
    let bytes = bytes.clamp(CHUNK as u32, MAX_BYTES) as usize;
    let chunks = bytes.div_ceil(CHUNK) as u32;
    tokio::spawn(async move {
        let pad = "w".repeat(CHUNK);
        for seq in 0..chunks {
            let msg = RelayMsg::LinkTestChunk {
                seq,
                pad: pad.clone(),
            };
            let Ok(t) = serde_json::to_string(&msg) else {
                return;
            };
            if out.send(t).await.is_err() {
                return;
            }
        }
        if let Ok(t) = serde_json::to_string(&RelayMsg::LinkTestDone { chunks }) {
            let _ = out.send(t).await;
        }
    });
}
