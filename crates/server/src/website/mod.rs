//! The wado control plane: an always-on HTTP **API** that lets a client configure
//! and **trigger** compositor sessions on demand, carries the WebRTC video for the
//! running session, and streams wado's logs back to the client.
//!
//! This server is API-only — the UI is a separate app (`wado-client`, a Dioxus web
//! app) that talks to these endpoints over CORS. The endpoints are:
//!   - `POST /session/start` — body is a `wado_protocol::SessionConfig` (JSON).
//!   - `POST /session/stop`  — tear the active session down.
//!   - `POST /offer`         — WebRTC SDP offer → answer (JSON).
//!   - `GET  /events`        — live tracing logs as Server-Sent Events.
//!   - `OPTIONS *`           — CORS preflight (204).
//!
//! wado boots into this server only — no EGL, no encoder, no render loop — so an
//! idle instance consumes ~no GPU/CPU. A session is created when the client POSTs
//! `/session/start`, and torn down on `/session/stop` or when the viewer's WebRTC
//! connection truly fails.
//!
//! ## Threading
//! The HTTP server is async (tokio, on its own thread); the compositor session
//! lives on the synchronous `calloop` main thread. They are bridged two ways:
//!   - **control**: a `calloop::channel` carries [`ControlCommand`]s (`Start` /
//!     `Stop` / `ForceKeyframe`) to the compositor; replies come back on a tokio
//!     `oneshot`.
//!   - **frames**: a bounded drop-on-full `tokio::mpsc` carries encoded frames from
//!     the session's `ChannelSink` to the WebRTC frame pump here.
//!
//! ## Robustness
//! - ICE timeouts are lengthened (disconnected 15 s) so transient blips don't drop
//!   the stream; mDNS stays at the default `QueryOnly` (correct for localhost).
//! - The browser's RTCP **PLI/FIR** drives `ForceKeyframe`, and a keyframe is forced
//!   when a viewer connects — so video recovers fast after loss and starts instantly.
//! - A **generation** guard stops a *stale* viewer's teardown from killing a newer
//!   session (e.g. across an ICE restart).
//!
//! ## Security (interim)
//! The launch command is free-form, so this server binds `127.0.0.1` by default.
//! A password/approval gate (and only then LAN exposure) is the next step.

pub mod control;
pub mod logbus;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use smithay::reexports::calloop::{
    LoopHandle,
    channel::{Event as ChannelEvent, channel},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{error, info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MediaEngine};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::{API, APIBuilder};
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use crate::Wado;
use crate::sink::channel::FrameMsg;
use control::ControlCommand;
use logbus::LogBus;

/// Bounded so encoded frames never pile up behind a slow/absent network.
const FRAME_CHANNEL_CAPACITY: usize = 4;
/// Reject oversized request bodies (SDP/config are tiny; this is a DoS guard).
const MAX_BODY_BYTES: usize = 256 * 1024;
/// How long `/session/start` waits for the compositor thread to reply.
const START_REPLY_TIMEOUT: Duration = Duration::from_secs(3);

/// `Send`able sender for [`ControlCommand`]s into the calloop loop.
type CmdSender = smithay::reexports::calloop::channel::Sender<ControlCommand>;

/// Per-connection shared context for the HTTP handlers.
struct ServerCtx {
    api: Arc<API>,
    track: Arc<TrackLocalStaticSample>,
    cmd_tx: CmdSender,
    log_bus: LogBus,
    /// The single active viewer's peer connection, kept alive here (not merely by
    /// the RTCP task) and cleared when it fails/closes.
    active_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
    /// Bumped on every accepted offer; lets a stale viewer's teardown be ignored.
    generation: Arc<AtomicU64>,
}

/// Start the control plane. Inserts the command source into `loop_handle` and
/// spawns the tokio runtime (HTTP server + frame pump) on its own thread.
pub fn start(
    loop_handle: LoopHandle<'static, Wado>,
    addr: &str,
    log_bus: LogBus,
) -> crate::Result<()> {
    let (frame_tx, frame_rx) = mpsc::channel::<FrameMsg>(FRAME_CHANNEL_CAPACITY);
    let (cmd_tx, cmd_channel) = channel::<ControlCommand>();

    // Compositor-side command handler (runs on the calloop thread, owns &mut Wado).
    loop_handle
        .insert_source(cmd_channel, move |event, _, state: &mut Wado| {
            if let ChannelEvent::Msg(cmd) = event {
                control::handle_command(state, cmd, &frame_tx);
            }
        })
        .map_err(|e| crate::WadoError::Other(format!("insert control source: {e}")))?;

    let addr = addr.to_string();
    std::thread::Builder::new()
        .name("wado-website".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("failed to build tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                if let Err(e) = run_server(addr, frame_rx, cmd_tx, log_bus).await {
                    error!("control server exited: {e}");
                }
            });
        })?;

    Ok(())
}

async fn run_server(
    addr: String,
    mut frame_rx: mpsc::Receiver<FrameMsg>,
    cmd_tx: CmdSender,
    log_bus: LogBus,
) -> crate::Result<()> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;

    // Forgiving ICE timeouts: a brief connectivity gap (default 5 s → disconnected)
    // must not drop a healthy localhost/LAN stream. mDNS stays at default QueryOnly.
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_ice_timeouts(
        Some(Duration::from_secs(15)), // disconnected
        Some(Duration::from_secs(30)), // failed
        Some(Duration::from_secs(2)),  // keepalive
    );

    let api = Arc::new(
        APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting_engine)
            .build(),
    );

    // One shared, persistent H.264 track fed by whichever session is running.
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability { mime_type: MIME_TYPE_H264.to_owned(), ..Default::default() },
        "video".to_owned(),
        "wado".to_owned(),
    ));

    // Frame pump: encoded frames → write_sample. Harmless no-op when no viewer.
    {
        let track = Arc::clone(&track);
        tokio::spawn(async move {
            while let Some((buf, dur)) = frame_rx.recv().await {
                let sample = Sample { data: Bytes::from(buf), duration: dur, ..Default::default() };
                if let Err(e) = track.write_sample(&sample).await {
                    warn!("write_sample error: {e}");
                }
            }
        });
    }

    let ctx = Arc::new(ServerCtx {
        api,
        track,
        cmd_tx,
        log_bus,
        active_pc: Arc::new(Mutex::new(None)),
        generation: Arc::new(AtomicU64::new(0)),
    });

    let listener = TcpListener::bind(&addr).await?;
    info!(%addr, "control server listening — connect with the wado-client app");

    loop {
        let (stream, _peer) = listener.accept().await?;
        let ctx = Arc::clone(&ctx);
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, ctx).await {
                warn!("connection error: {e}");
            }
        });
    }
}

async fn handle_conn(mut stream: TcpStream, ctx: Arc<ServerCtx>) -> crate::Result<()> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];

    let header_end = loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > 64 * 1024 {
            write_response(&mut stream, "431 Request Header Fields Too Large", "text/plain", b"")
                .await?;
            return Ok(());
        }
    };

    let header_text = String::from_utf8_lossy(&buf[..header_end]);
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > MAX_BODY_BYTES {
        write_response(&mut stream, "413 Payload Too Large", "text/plain", b"body too large")
            .await?;
        return Ok(());
    }

    // CORS preflight: the client is a separate origin, so browsers preflight the
    // JSON POSTs. Answer any OPTIONS with the allowed methods/headers. No body.
    if method == "OPTIONS" {
        return write_preflight(&mut stream).await;
    }

    // Live log stream is a long-lived response — handle before the normal path.
    if method == "GET" && path == "/events" {
        return serve_sse(stream, &ctx.log_bus).await;
    }

    let body_start = header_end + 4;
    let mut body = buf[body_start..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/") => {
            // No UI here anymore — the client is the separate `wado-client` app.
            let msg = b"wado control server (API only). Run the wado-client app to connect.";
            write_response(&mut stream, "200 OK", "text/plain", msg).await?;
        }
        ("POST", "/session/start") => match handle_session_start(&ctx, &body).await {
            Ok(()) => write_response(&mut stream, "200 OK", "text/plain", b"started").await?,
            Err(e) => {
                warn!("session start rejected: {e}");
                write_response(&mut stream, "409 Conflict", "text/plain", e.as_bytes()).await?
            }
        },
        ("POST", "/session/stop") => {
            let _ = ctx.cmd_tx.send(ControlCommand::Stop);
            write_response(&mut stream, "200 OK", "text/plain", b"stopped").await?;
        }
        ("POST", "/offer") => {
            let offer_json = String::from_utf8_lossy(&body);
            match handle_offer(&ctx, &offer_json).await {
                Ok(answer) => {
                    write_response(&mut stream, "200 OK", "application/json", answer.as_bytes())
                        .await?
                }
                Err(e) => {
                    error!("offer handling failed: {e}");
                    write_response(&mut stream, "500 Internal Server Error", "text/plain", b"offer failed")
                        .await?
                }
            }
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await?,
    }

    Ok(())
}

/// Parse a `SessionConfig` and ask the compositor thread to start a session,
/// bounded by a timeout so a wedged compositor can't hang the HTTP connection.
async fn handle_session_start(ctx: &ServerCtx, body: &[u8]) -> std::result::Result<(), String> {
    let config = serde_json::from_slice(body).map_err(|e| format!("bad config: {e}"))?;
    let (reply_tx, reply_rx) = oneshot::channel();
    ctx.cmd_tx
        .send(ControlCommand::Start { config, reply: reply_tx })
        .map_err(|_| "compositor unavailable".to_string())?;
    match tokio::time::timeout(START_REPLY_TIMEOUT, reply_rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("compositor dropped reply".into()),
        Err(_) => Err("session start timed out".into()),
    }
}

/// Build a peer connection for one viewer, attach the shared track, wire RTCP
/// PLI→keyframe, and answer. A generation tag prevents a stale viewer's teardown
/// from killing a session started later.
async fn handle_offer(ctx: &ServerCtx, offer_json: &str) -> crate::Result<String> {
    let offer: RTCSessionDescription = serde_json::from_str(offer_json)?;

    // No STUN: this is a localhost/LAN stream; host candidates are enough and
    // skipping STUN gathering removes latency + an external dependency.
    let pc = Arc::new(ctx.api.new_peer_connection(RTCConfiguration::default()).await?);

    let rtp_sender = pc
        .add_track(Arc::clone(&ctx.track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // This viewer's generation; only this generation may auto-stop the session.
    let my_gen = ctx.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *ctx.active_pc.lock().unwrap() = Some(Arc::clone(&pc));

    // RTCP read loop: the browser sends PLI/FIR when it needs a keyframe.
    let cmd_tx_rtcp = ctx.cmd_tx.clone();
    tokio::spawn(async move {
        loop {
            match rtp_sender.read_rtcp().await {
                Ok((packets, _)) => {
                    for p in packets {
                        let any = p.as_any();
                        if any.downcast_ref::<PictureLossIndication>().is_some()
                            || any.downcast_ref::<FullIntraRequest>().is_some()
                        {
                            let _ = cmd_tx_rtcp.send(ControlCommand::ForceKeyframe);
                        }
                    }
                }
                Err(_) => break, // sender closed
            }
        }
    });

    let cmd_tx = ctx.cmd_tx.clone();
    let generation = Arc::clone(&ctx.generation);
    let active_pc = Arc::clone(&ctx.active_pc);
    pc.on_peer_connection_state_change(Box::new(move |state| {
        info!("viewer connection state: {state}");
        match state {
            // Force an IDR so the new viewer gets a picture immediately.
            RTCPeerConnectionState::Connected => {
                let _ = cmd_tx.send(ControlCommand::ForceKeyframe);
            }
            // Only the current viewer tears the session down (generation guard).
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                if generation.load(Ordering::SeqCst) == my_gen {
                    info!("viewer gone — stopping session");
                    let _ = cmd_tx.send(ControlCommand::Stop);
                    *active_pc.lock().unwrap() = None;
                }
            }
            _ => {}
        }
        Box::pin(async {})
    }));

    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;

    let local = pc
        .local_description()
        .await
        .ok_or_else(|| crate::WadoError::Other("no local description after gathering".into()))?;
    Ok(serde_json::to_string(&local)?)
}

/// Stream wado's live tracing output to a `/events` listener as Server-Sent Events.
async fn serve_sse(mut stream: TcpStream, log_bus: &LogBus) -> crate::Result<()> {
    let header = "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: keep-alive\r\n\r\n";
    stream.write_all(header.as_bytes()).await?;

    // Backfill recent history so a freshly opened panel isn't empty.
    for line in log_bus.backfill() {
        stream.write_all(format!("data: {line}\n\n").as_bytes()).await?;
    }
    stream.flush().await?;

    let mut rx = log_bus.subscribe();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(line) => {
                    if stream.write_all(format!("data: {line}\n\n").as_bytes()).await.is_err() {
                        break; // client disconnected
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = heartbeat.tick() => {
                // Comment line; also surfaces a dead socket as a write error.
                if stream.write_all(b": keepalive\n\n").await.is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Answer a CORS preflight (`OPTIONS`) with the methods/headers the client needs.
async fn write_preflight(stream: &mut TcpStream) -> crate::Result<()> {
    let header = "HTTP/1.1 204 No Content\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Access-Control-Max-Age: 86400\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\r\n";
    stream.write_all(header.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> crate::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
