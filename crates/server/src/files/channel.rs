//! The files peer connection: opened on demand by a `files_offer` that came through the sealed
//! envelope (so its DTLS fingerprint is authenticated), separate from the session's so a
//! transfer never shares an SCTP association with input (invariant #1). One ordered, reliable
//! data channel, `files`, carries `wado_protocol::files`.
//!
//! **Who may do what** is checked per request, not once at open: a grant revoked with
//! `wado files grant <device> none` applies to the next request of a channel already open.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use serde_json::{Value, json};
use tracing::{info, warn};
use webrtc::api::API;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use wado_protocol::files::{FILES_CHANNEL, FileFrame, FileReq};

use crate::gate::Gate;

/// Above this much queued in the channel, a sender waits.
const HIGH_WATER: usize = 1 << 20;

/// The device on the other end, as the envelope proved it.
#[derive(Clone)]
pub struct Who {
    pub key: String,
    pub name: String,
}

/// Why `who` may not open files at all, or `None` when it may.
pub fn refused(gate: &Gate, who: &Who, once: bool) -> Option<String> {
    if !wado_config::live::current().files.enabled {
        return Some("file access is turned off on this computer (files { enabled })".into());
    }
    if once {
        return Some("a device let in \"once\" gets no file access".into());
    }
    match gate.entry(&who.key) {
        Some((_, false)) => Some(
            "file access needs this device paired by QR — scan the computer's code (`wado qr`)"
                .into(),
        ),
        _ if gate.files_access(&who.key) == "none" => Some(format!(
            "this device has no file access — on the computer run: wado files grant \"{}\" rw",
            who.name
        )),
        _ => None,
    }
}

/// Build the files peer connection for `offer` and return it with the answer to send back.
pub async fn open(
    api: &API,
    offer: &str,
    who: Who,
    gate: Gate,
) -> crate::Result<(Arc<RTCPeerConnection>, String)> {
    let offer: RTCSessionDescription = serde_json::from_str(offer)
        .map_err(|e| crate::WadoError::Other(format!("bad files SDP: {e}")))?;
    let pc = Arc::new(
        api.new_peer_connection(RTCConfiguration {
            ice_servers: crate::ice::servers(),
            ..Default::default()
        })
        .await?,
    );
    pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let (who, gate) = (who.clone(), gate.clone());
        Box::pin(async move {
            if dc.label() == FILES_CHANNEL {
                info!(device = %who.name, "files: channel open");
                serve(dc, who, gate);
            }
        })
    }));
    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    let _ = tokio::time::timeout(crate::ice::GATHER_WAIT, gather.recv()).await;
    let local = pc
        .local_description()
        .await
        .ok_or_else(|| crate::WadoError::Other("no local description".into()))?;
    Ok((pc, serde_json::to_string(&local)?))
}

/// One open channel: its device, and its transfers in flight.
pub struct Chan {
    dc: Arc<RTCDataChannel>,
    pub who: Who,
    gate: Gate,
    /// Downloads, by transfer id: set to stop them.
    stops: Mutex<HashMap<u32, Arc<AtomicBool>>>,
    /// Uploads, by transfer id.
    pub ups: Mutex<HashMap<u32, super::recv::Upload>>,
    /// Media streams' credit — bytes the device has said it will take — by transfer id.
    credits: Mutex<HashMap<u32, Arc<AtomicU64>>>,
}

impl Chan {
    pub async fn reply(&self, id: u32, mut v: Value) {
        if let Some(o) = v.as_object_mut() {
            o.insert("id".into(), id.into());
            if !o.contains_key("err") && !o.contains_key("done") {
                o.insert("ok".into(), true.into());
            }
        }
        let _ = self.dc.send_text(v.to_string()).await;
    }

    pub async fn fail(&self, id: u32, why: impl Into<String>) {
        self.reply(id, json!({ "err": why.into() })).await;
    }

    /// One binary frame for transfer `id`, once the channel has room and the pace allows.
    /// False once the channel is gone.
    pub async fn bytes(&self, id: u32, data: &[u8]) -> bool {
        while self.dc.buffered_amount().await > HIGH_WATER {
            if self.closed() {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        super::pace::take(data.len()).await;
        let mut frame = Vec::with_capacity(4 + data.len());
        frame.extend_from_slice(&id.to_be_bytes());
        frame.extend_from_slice(data);
        self.dc.send(&Bytes::from(frame)).await.is_ok()
    }

    pub fn closed(&self) -> bool {
        use webrtc::data_channel::data_channel_state::RTCDataChannelState;
        self.dc.ready_state() != RTCDataChannelState::Open
    }

    /// A stop flag for a new download.
    pub fn start(&self, id: u32) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        self.stops
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, Arc::clone(&stop));
        stop
    }

    pub fn finish(&self, id: u32) {
        self.stops
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }

    /// `none`, `ro` or `rw`, as of now.
    pub fn access(&self) -> &'static str {
        if !wado_config::live::current().files.enabled {
            return "none";
        }
        self.gate.files_access(&self.who.key)
    }

    pub fn audit(&self, op: &str, path: &str, size: u64, result: &Result<(), String>) {
        super::audit::record(&self.who, op, path, size, result);
    }
}

fn serve(dc: Arc<RTCDataChannel>, who: Who, gate: Gate) {
    let chan = Arc::new(Chan {
        dc: Arc::clone(&dc),
        who,
        gate,
        stops: Mutex::new(HashMap::new()),
        ups: Mutex::new(HashMap::new()),
        credits: Mutex::new(HashMap::new()),
    });
    // Weak in the channel's own callbacks: `Chan` holds the channel, and a strong reference
    // back would keep both alive forever.
    let closing = Arc::downgrade(&chan);
    dc.on_close(Box::new(move || {
        if let Some(c) = closing.upgrade() {
            for stop in c.stops.lock().unwrap_or_else(|e| e.into_inner()).values() {
                stop.store(true, Ordering::SeqCst);
            }
            info!(device = %c.who.name, "files: channel closed");
            // Drop the message callback, and with it the last strong reference to `Chan`.
            c.dc.on_message(Box::new(|_| Box::pin(async {})));
        }
        Box::pin(async {})
    }));
    // The message callback is the one strong owner, until the close above replaces it.
    dc.on_message(Box::new(move |msg: DataChannelMessage| {
        let chan = Arc::clone(&chan);
        Box::pin(async move {
            if !msg.is_string {
                // Upload bytes: written here, in order, before the next frame is read.
                super::recv::chunk(&chan, &msg.data).await;
                return;
            }
            match serde_json::from_slice::<FileFrame>(&msg.data) {
                Ok(f) => dispatch(chan, f.id, f.req).await,
                Err(e) => warn!("files: bad request: {e}"),
            }
        })
    }));
}

/// Is `req` allowed at access level `have`?
fn allowed(req: &FileReq, have: &str) -> bool {
    use FileReq::*;
    match req {
        Hello | Cancel { .. } => true,
        List { .. }
        | Get { .. }
        | Zip { .. }
        | Quick
        | Thumb { .. }
        | Probe { .. }
        | Stream { .. }
        | Credit { .. }
        | Subs { .. } => have != "none",
        _ => have == "rw",
    }
}

async fn dispatch(chan: Arc<Chan>, id: u32, req: FileReq) {
    let have = chan.access();
    if !allowed(&req, have) {
        let why = if have == "none" {
            "this device no longer has file access"
        } else {
            "this device may only read files here (ro)"
        };
        return chan.fail(id, why).await;
    }
    let sc = super::scope();
    // Uploads are answered here, in order, so their bytes cannot arrive before the upload exists.
    if let FileReq::Put {
        dir,
        name,
        size,
        clash,
    } = req
    {
        return super::recv::put(&chan, &sc, id, &dir, &name, size, clash).await;
    }
    // Everything else runs on its own: a long copy must not hold up a listing.
    tokio::spawn(async move {
        use FileReq::*;
        match req {
            Hello => {
                let roots: Vec<_> = sc
                    .roots()
                    .iter()
                    .map(|r| r.to_string_lossy().into_owned())
                    .collect();
                let home = super::scope::home().to_string_lossy().into_owned();
                // Room on each root's disk, for the storage meter.
                let space: Vec<_> = sc
                    .roots()
                    .iter()
                    .filter_map(|r| {
                        let (free, total) = super::space(r)?;
                        Some(json!({ "root": r.to_string_lossy(), "free": free, "total": total }))
                    })
                    .collect();
                // The Trash, when the scope reaches it, for "Trash" in the sidebar.
                let trash = super::trash::home_trash().join("files");
                let trash = sc
                    .dir(&trash.to_string_lossy())
                    .ok()
                    .map(|_| trash.to_string_lossy().into_owned());
                chan.reply(id, json!({ "roots": roots, "home": home, "access": have, "device": chan.who.name, "space": space, "trash": trash }))
                    .await;
            }
            List { path } => match super::list::list(&sc, &path) {
                Ok(v) => chan.reply(id, v).await,
                Err(e) => chan.fail(id, e).await,
            },
            Get { path, offset } => super::send::get(&chan, &sc, id, &path, offset).await,
            Zip { path } => super::zip::send(&chan, &sc, id, &path).await,
            PutEnd { xfer, sha256 } => super::recv::end(&chan, id, xfer, &sha256).await,
            Cancel { xfer } => {
                if let Some(s) = chan
                    .stops
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&xfer)
                {
                    s.store(true, Ordering::SeqCst);
                }
                // An upload keeps its `.wado-part` for a resume; only the open file goes.
                chan.ups
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&xfer);
                chan.reply(id, json!({})).await;
            }
            Mkdir { path } => done(&chan, id, "mkdir", &path, super::ops::mkdir(&sc, &path)).await,
            Rename { path, to } => {
                done(
                    &chan,
                    id,
                    "rename",
                    &path,
                    super::ops::rename(&sc, &path, &to),
                )
                .await
            }
            Trash { paths } => {
                many(&chan, id, "trash", &paths, |p| super::trash::trash(&sc, p)).await
            }
            Move { paths, dest, clash } => {
                many(&chan, id, "move", &paths, |p| {
                    super::ops::transfer(&sc, p, &dest, clash, false)
                })
                .await
            }
            Copy { paths, dest, clash } => {
                many(&chan, id, "copy", &paths, |p| {
                    super::ops::transfer(&sc, p, &dest, clash, true)
                })
                .await
            }
            Quick => chan.reply(id, super::quick::quick(&sc)).await,
            Thumb { path } => match super::thumb::thumb(&sc, &path).await {
                Ok(b64) => chan.reply(id, json!({ "png": b64 })).await,
                Err(e) => chan.fail(id, e).await,
            },
            Devices => {
                let list: Vec<_> = chan
                    .gate
                    .devices()
                    .into_iter()
                    .map(|(key, name, pinned, files)| {
                        json!({ "key": key, "name": name, "pinned": pinned, "files": files, "me": key == chan.who.key })
                    })
                    .collect();
                chan.reply(id, json!({ "devices": list })).await;
            }
            Grant { key, level } => {
                let r = chan.gate.grant(&key, &level).map(|_| ());
                chan.audit("grant", &format!("{key} {level}"), 0, &r);
                match r {
                    Ok(()) => chan.reply(id, json!({})).await,
                    Err(e) => chan.fail(id, e).await,
                }
            }
            Pin { path } => {
                done(&chan, id, "pin", &path, super::quick::pin(&sc, &path, true)).await
            }
            Unpin { path } => {
                done(
                    &chan,
                    id,
                    "unpin",
                    &path,
                    super::quick::pin(&sc, &path, false),
                )
                .await
            }
            Find { path, query, kind } => {
                let r = tokio::task::spawn_blocking(move || {
                    super::find::find(&sc, &path, &query, &kind)
                })
                .await;
                match r {
                    Ok(v) => chan.reply(id, v).await,
                    Err(e) => chan.fail(id, e.to_string()).await,
                }
            }
            Restore { path } => {
                let r = super::trash::restore(&sc, &path);
                chan.audit(
                    "restore",
                    &path,
                    0,
                    &r.as_ref().map(|_| ()).map_err(Clone::clone),
                );
                match r {
                    Ok(to) => chan.reply(id, json!({ "to": to })).await,
                    Err(e) => chan.fail(id, e).await,
                }
            }
            Probe { path } => match super::media::probe(&sc, &path).await {
                Ok(v) => chan.reply(id, v).await,
                Err(e) => chan.fail(id, e).await,
            },
            Stream {
                path,
                start,
                audio,
                hevc,
                credit,
            } => {
                let c = Arc::new(AtomicU64::new(credit.max(1 << 20)));
                chan.credits
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id, Arc::clone(&c));
                super::media::stream(&chan, &sc, id, &path, start, audio, hevc, c).await;
                chan.credits
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&id);
                // Watching is not worth a line per seek: once per file, at its start.
                if start == 0.0 {
                    chan.audit("play", &path, 0, &Ok(()));
                }
            }
            Credit { xfer, bytes } => {
                if let Some(c) = chan
                    .credits
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&xfer)
                {
                    c.fetch_add(bytes, Ordering::SeqCst);
                }
                chan.reply(id, json!({})).await;
            }
            Subs {
                path,
                track,
                sidecar,
            } => match super::media::subs(&sc, &path, track, sidecar.as_deref()).await {
                Ok(vtt) => chan.reply(id, json!({ "vtt": vtt })).await,
                Err(e) => chan.fail(id, e).await,
            },
            Put { .. } => unreachable!("answered above"),
        }
    });
}

/// Run `op` on each path, auditing each; stop at the first failure and say which.
async fn many(
    chan: &Chan,
    id: u32,
    name: &str,
    paths: &[String],
    op: impl Fn(&str) -> Result<(), String>,
) {
    for p in paths {
        let r = op(p);
        chan.audit(name, p, 0, &r);
        if let Err(e) = r {
            let base = p.rsplit('/').next().unwrap_or(p);
            return chan.fail(id, format!("{base}: {e}")).await;
        }
    }
    chan.reply(id, json!({})).await;
}

/// Audit a one-path operation and answer it.
async fn done(chan: &Chan, id: u32, op: &str, path: &str, r: Result<(), String>) {
    chan.audit(op, path, 0, &r);
    match r {
        Ok(()) => chan.reply(id, json!({})).await,
        Err(e) => chan.fail(id, e).await,
    }
}
