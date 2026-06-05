//! The wado control plane: an always-on HTTP server that serves the native web
//! client, lets it configure and **trigger** compositor sessions on demand, and
//! carries the WebRTC video for the running session.
//!
//! wado boots into this server only — no EGL, no encoder, no render loop — so an
//! idle instance consumes ~no GPU/CPU. A session is created when the web client
//! POSTs `/session/start`, and torn down on `/session/stop` or when the viewer's
//! WebRTC connection closes.
//!
//! ## Threading
//! The HTTP server is async (tokio, on its own thread); the compositor session
//! lives on the synchronous `calloop` main thread. They are bridged two ways:
//!   - **control**: a `calloop::channel` carries [`ControlCommand`]s from HTTP
//!     handlers to the compositor (`Start`/`Stop`); replies come back on a tokio
//!     `oneshot`.
//!   - **frames**: a bounded drop-on-full `tokio::mpsc` carries encoded frames
//!     from the session's `ChannelSink` to the WebRTC frame pump here.
//!
//! ## Security (interim)
//! The launch command is free-form, so this server binds `127.0.0.1` by default.
//! A password/approval gate (and only then LAN exposure) is the next step.

pub mod control;

use std::sync::Arc;

use bytes::Bytes;
use smithay::reexports::calloop::{
    LoopHandle,
    channel::{Event as ChannelEvent, channel},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MediaEngine};
use webrtc::api::{API, APIBuilder};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use crate::Wado;
use crate::sink::channel::FrameMsg;
use control::ControlCommand;

/// Boxed send-safe error for the async tasks.
type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// Bounded so encoded frames never pile up behind a slow/absent network.
const FRAME_CHANNEL_CAPACITY: usize = 4;

/// `Send`able sender for [`ControlCommand`]s into the calloop loop.
type CmdSender = smithay::reexports::calloop::channel::Sender<ControlCommand>;

/// Per-connection shared context for the HTTP handlers.
struct ServerCtx {
    api: Arc<API>,
    track: Arc<TrackLocalStaticSample>,
    cmd_tx: CmdSender,
}

/// Start the control plane. Inserts the command source into `loop_handle` and
/// spawns the tokio runtime (HTTP server + frame pump) on its own thread.
pub fn start(loop_handle: LoopHandle<'static, Wado>, addr: &str) -> Result<(), BoxErr> {
    let (frame_tx, frame_rx) = mpsc::channel::<FrameMsg>(FRAME_CHANNEL_CAPACITY);
    let (cmd_tx, cmd_channel) = channel::<ControlCommand>();

    // Compositor-side command handler (runs on the calloop thread, owns &mut Wado).
    loop_handle
        .insert_source(cmd_channel, move |event, _, state: &mut Wado| {
            if let ChannelEvent::Msg(cmd) = event {
                control::handle_command(state, cmd, &frame_tx);
            }
        })
        .map_err(|e| format!("failed to insert control source: {e}"))?;

    let addr = addr.to_string();
    std::thread::Builder::new()
        .name("wado-website".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("[website] failed to build tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                if let Err(e) = run_server(addr, frame_rx, cmd_tx).await {
                    eprintln!("[website] control server exited: {e}");
                }
            });
        })?;

    Ok(())
}

async fn run_server(
    addr: String,
    mut frame_rx: mpsc::Receiver<FrameMsg>,
    cmd_tx: CmdSender,
) -> Result<(), BoxErr> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;
    let api = Arc::new(
        APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
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
                    eprintln!("[website] write_sample error: {e}");
                }
            }
        });
    }

    let ctx = Arc::new(ServerCtx { api, track, cmd_tx });

    let listener = TcpListener::bind(&addr).await?;
    eprintln!("[website] control server on http://{addr}  — open it in a browser");

    loop {
        let (stream, _peer) = listener.accept().await?;
        let ctx = Arc::clone(&ctx);
        tokio::spawn(async move {
            if let Err(e) = handle_conn(stream, ctx).await {
                eprintln!("[website] connection error: {e}");
            }
        });
    }
}

async fn handle_conn(mut stream: TcpStream, ctx: Arc<ServerCtx>) -> Result<(), BoxErr> {
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
            let html = include_str!("../../assets/index.html");
            write_response(&mut stream, "200 OK", "text/html; charset=utf-8", html.as_bytes())
                .await?;
        }
        ("POST", "/session/start") => {
            let resp = handle_session_start(&ctx, &body).await;
            match resp {
                Ok(()) => write_response(&mut stream, "200 OK", "text/plain", b"started").await?,
                Err(e) => {
                    write_response(&mut stream, "409 Conflict", "text/plain", e.as_bytes()).await?
                }
            }
        }
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
                    eprintln!("[website] offer handling failed: {e}");
                    write_response(&mut stream, "500 Internal Server Error", "text/plain", b"offer failed")
                        .await?
                }
            }
        }
        _ => write_response(&mut stream, "404 Not Found", "text/plain", b"not found").await?,
    }

    Ok(())
}

/// Parse a `SessionConfig` and ask the compositor thread to start a session.
async fn handle_session_start(ctx: &ServerCtx, body: &[u8]) -> Result<(), String> {
    let config = serde_json::from_slice(body).map_err(|e| format!("bad config: {e}"))?;
    let (reply_tx, reply_rx) = oneshot::channel();
    ctx.cmd_tx
        .send(ControlCommand::Start { config, reply: reply_tx })
        .map_err(|_| "compositor unavailable".to_string())?;
    reply_rx.await.map_err(|_| "compositor dropped reply".to_string())?
}

/// Build a peer connection for one viewer, attach the shared track, and answer.
/// On disconnect, request a session teardown so resources free automatically.
async fn handle_offer(ctx: &ServerCtx, offer_json: &str) -> Result<String, BoxErr> {
    let offer: RTCSessionDescription = serde_json::from_str(offer_json)?;

    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let pc = Arc::new(ctx.api.new_peer_connection(config).await?);

    let rtp_sender = pc
        .add_track(Arc::clone(&ctx.track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    let pc_keepalive = Arc::clone(&pc);
    tokio::spawn(async move {
        let _pc = pc_keepalive;
        let mut rtcp_buf = vec![0u8; 1500];
        while rtp_sender.read(&mut rtcp_buf).await.is_ok() {}
    });

    // Auto-teardown: when the viewer's connection ends, stop the session.
    let cmd_tx = ctx.cmd_tx.clone();
    pc.on_peer_connection_state_change(Box::new(move |state| {
        eprintln!("[website] peer connection state: {state}");
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            let _ = cmd_tx.send(ControlCommand::Stop);
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
        .ok_or("no local description after gathering")?;
    Ok(serde_json::to_string(&local)?)
}

async fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), BoxErr> {
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
