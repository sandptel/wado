//! Files with no direct path: the same requests and answers as the files channel
//! (`super::channel`), carried sealed on the relay link instead — the fallback lane, 64 KB/s
//! (Decision Log 2026-10-27: "listings and files ≤ 1 MB on the fallback lane; thumbnails and
//! big transfers wait for Direct").
//!
//! What waits for a direct path, and says so: thumbnails, folder archives, playback, and any
//! transfer over [`MAX_BYTES`]. The refusal carries `"lane": true`, so the phone shows it as the
//! network's doing — with Retry direct — not as an error of wado's.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn};
use wado_protocol::files::{FileFrame, FileReq};
use wado_protocol::relay::RelayMsg;

use super::channel::{self, Chan, Who};
use crate::gate::Gate;
use crate::lane::Gauge;

/// The most one transfer may move on the lane: about sixteen seconds of it.
pub const MAX_BYTES: u64 = 1 << 20;

/// Why a transfer is refused on the lane.
pub fn too_big(size: u64) -> String {
    format!(
        "{:.1} MB is more than the relay's limited lane carries (files up to 1 MB go without a \
         direct connection). This is the network, not wado — Retry direct, or switch off a VPN \
         or use mobile data.",
        size as f64 / 1_048_576.0
    )
}

const NEEDS_DIRECT: &str = "this needs a direct connection, and there is none — only the \
     relay's limited lane is up. This is the network, not wado: Retry direct, or switch off a VPN \
     or use mobile data.";

/// What the lane will not carry at all.
fn heavy(req: &FileReq) -> bool {
    matches!(
        req,
        FileReq::Zip { .. }
            | FileReq::Thumb { .. }
            | FileReq::Stream { .. }
            | FileReq::Subs { .. }
            | FileReq::Probe { .. }
    )
}

/// A channel's way out on the lane.
#[derive(Clone)]
pub struct LaneTx {
    out: mpsc::Sender<String>,
    gauge: Gauge,
    alive: Arc<AtomicBool>,
}

impl LaneTx {
    pub async fn text(&self, text: String) {
        if let Ok(t) = serde_json::to_string(&RelayMsg::FilesOut { text }) {
            let _ = self.out.send(t).await;
        }
    }

    /// One chunk of transfer `id`, once the lane has room for it.
    pub async fn bytes(&self, id: u32, data: &[u8]) -> bool {
        self.gauge.room().await;
        if self.closed() {
            return false;
        }
        let msg = RelayMsg::FilesChunk {
            id,
            b: STANDARD.encode(data),
        };
        match serde_json::to_string(&msg) {
            Ok(t) => self.out.send(t).await.is_ok(),
            Err(_) => false,
        }
    }

    pub fn closed(&self) -> bool {
        !self.alive.load(Ordering::SeqCst) || self.out.is_closed()
    }
}

/// The files channel on one relay link, made on its first request.
#[derive(Default)]
pub struct OnLane {
    chan: Mutex<Option<(Arc<Chan>, Arc<AtomicBool>)>>,
}

/// Who is asking, and whether the gate has let them in.
pub struct Asker<'a> {
    pub who: Who,
    pub gate: &'a Gate,
    pub ok: bool,
    pub out: &'a mpsc::Sender<String>,
    pub gauge: &'a Gauge,
}

impl OnLane {
    fn get(&self, a: &Asker<'_>) -> Result<Arc<Chan>, String> {
        let mut g = self.chan.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((c, _)) = g.as_ref()
            && c.who.key == a.who.key
        {
            return Ok(Arc::clone(c));
        }
        if !a.ok {
            return Err("this device has not been let in yet".into());
        }
        if let Some(why) = channel::refused(a.gate, &a.who) {
            return Err(why);
        }
        if let Some((old, alive)) = g.take() {
            alive.store(false, Ordering::SeqCst);
            old.stop_all();
        }
        let alive = Arc::new(AtomicBool::new(true));
        let tx = LaneTx {
            out: a.out.clone(),
            gauge: a.gauge.clone(),
            alive: Arc::clone(&alive),
        };
        info!(device = %a.who.name, "files: on the relay's lane — no direct path");
        let c = Arc::new(Chan::on_lane(tx, a.who.clone(), a.gate.clone()));
        *g = Some((Arc::clone(&c), alive));
        Ok(c)
    }

    /// One request on the lane.
    pub async fn request(&self, text: &str, a: Asker<'_>) {
        let frame = match serde_json::from_str::<FileFrame>(text) {
            Ok(f) => f,
            Err(e) => return warn!("files: bad request on the lane: {e}"),
        };
        let chan = match self.get(&a) {
            Ok(c) => c,
            Err(why) => {
                let t = json!({ "id": frame.id, "err": why }).to_string();
                if let Ok(m) = serde_json::to_string(&RelayMsg::FilesOut { text: t }) {
                    let _ = a.out.send(m).await;
                }
                return;
            }
        };
        if heavy(&frame.req) {
            return chan
                .reply(frame.id, json!({ "err": NEEDS_DIRECT, "lane": true }))
                .await;
        }
        channel::dispatch(chan, frame.id, frame.req).await;
    }

    /// One upload chunk on the lane.
    pub async fn chunk(&self, id: u32, b: &str) {
        let chan = self
            .chan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|(c, _)| Arc::clone(c));
        let (Some(chan), Ok(data)) = (chan, STANDARD.decode(b)) else {
            return;
        };
        let mut frame = Vec::with_capacity(4 + data.len());
        frame.extend_from_slice(&id.to_be_bytes());
        frame.extend_from_slice(&data);
        super::recv::chunk(&chan, &frame).await;
    }

    /// The relay link ended: whatever was moving on it stops.
    pub fn close(&self) {
        if let Some((c, alive)) = self.chan.lock().unwrap_or_else(|e| e.into_inner()).take() {
            alive.store(false, Ordering::SeqCst);
            c.stop_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_waits_for_a_direct_path() {
        assert!(heavy(&FileReq::Thumb { path: "/x".into() }));
        assert!(heavy(&FileReq::Zip { path: "/x".into() }));
        assert!(!heavy(&FileReq::List { path: "/x".into() }));
        assert!(!heavy(&FileReq::Hello));
        assert!(too_big(3 << 20).contains("3.0 MB") && too_big(3 << 20).contains("not wado"));
    }
}
