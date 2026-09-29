//! The wado control plane: an always-on HTTP **API** that lets a client configure
//! and **trigger** compositor sessions on demand, carries the WebRTC video for the
//! running session, and streams wado's logs back to the client.
//!
//! This server is API-only — the UI is a separate app (`wado-client`, a Dioxus web
//! app) that talks to these endpoints over CORS. The endpoints are:
//!   - `POST /session/start` — body is a `wado_protocol::SessionConfig` (JSON).
//!   - `POST /session/stop`  — tear the active session down.
//!   - `GET  /apps` — the launchable applications found on this machine.
//!   - `POST /session/control` — a JSON `SessionControl`: launch a command into the running session.
//!   - `POST /offer`         — WebRTC SDP offer → answer (JSON).
//!   - `GET  /events`        — live tracing logs as Server-Sent Events.
//!   - `GET  /timing`        — per-stage render timings (`StageTimings`, JSON).
//!   - `GET  /`              — plain-text hint that this server is API-only.
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
//!   - **control**: a `calloop::channel` (owned by `wado_compositor`) carries
//!     [`CompositorCommand`]s (`Start` / `Stop` / `ForceKeyframe`) to the compositor;
//!     replies come back on a tokio `oneshot`.
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

mod http;
pub mod logbus;
mod offer;
mod pump;
mod routes;
mod sse;
mod watchdog;

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MediaEngine};
use webrtc::api::{API, APIBuilder};
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use logbus::LogBus;
use wado_compositor::{CommandSender, FrameMsg, InputSender};
use wado_protocol::StageTimings;

/// Bounded so encoded frames never pile up behind a slow/absent network.
///
/// Kept deliberately shallow: every queued frame is latency the viewer will eventually
/// see, and for an interactive stream a stale frame is worth less than a fresh one. Two
/// slots absorb a single scheduling hiccup between the render tick and the pump without
/// letting a standing backlog form (4 slots at 60 fps was up to ~66 ms of queue).
pub const FRAME_CHANNEL_CAPACITY: usize = 2;

/// `Send`able sender for [`CompositorCommand`]s into the compositor's calloop loop.
type CmdSender = CommandSender;

/// Per-connection shared context for the HTTP handlers.
struct ServerCtx {
    api: Arc<API>,
    track: Arc<TrackLocalStaticSample>,
    cmd_tx: CmdSender,
    /// Sender for remote touch/keyboard events, fed by each viewer's input data channel.
    input_tx: InputSender,
    log_bus: LogBus,
    /// The single active viewer's peer connection, kept alive here (not merely by
    /// the RTCP task) and cleared when it fails/closes.
    active_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
    /// Bumped on every accepted offer; lets a stale viewer's teardown be ignored.
    generation: Arc<AtomicU64>,
    /// Latest per-stage render timings published by the compositor.
    timings: watch::Receiver<StageTimings>,
    /// Smoothed time (microseconds) encoded frames wait before the pump takes them.
    /// Lives here rather than in the compositor because only this side knows when the
    /// frame was actually picked up. Atomic so the pump task and the HTTP handlers can
    /// share it without a lock on the hot path.
    queue_us: Arc<AtomicU64>,
    /// Unix-millis of the last HTTP request from a viewer. See [`watchdog::viewer_watchdog`].
    last_request: Arc<AtomicU64>,
    /// Whether a session is running and has not been stopped. See [`watchdog::viewer_watchdog`].
    session_started: Arc<AtomicBool>,
}

/// Now, in unix milliseconds.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Start the control plane: spawn the tokio runtime (HTTP server + frame pump) on its
/// own thread. The compositor owns the calloop loop; this server only holds the command
/// `Sender` (to drive sessions) and the frame `Receiver` (the WebRTC pump) — it never
/// touches `Wado` or any Smithay type. Both channels are created by the caller (`main`)
/// and handed to [`wado_compositor::build`] and here respectively.
pub fn start(
    cmd_tx: CommandSender,
    input_tx: InputSender,
    frame_rx: mpsc::Receiver<FrameMsg>,
    timings: watch::Receiver<StageTimings>,
    addr: &str,
    log_bus: LogBus,
) -> crate::Result<()> {
    let addr = addr.to_string();
    std::thread::Builder::new()
        .name("wado-website".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    error!("failed to build tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                if let Err(e) = run_server(addr, frame_rx, cmd_tx, input_tx, timings, log_bus).await
                {
                    error!("control server exited: {e}");
                }
            });
        })?;

    Ok(())
}

async fn run_server(
    addr: String,
    frame_rx: mpsc::Receiver<FrameMsg>,
    cmd_tx: CmdSender,
    input_tx: InputSender,
    timings: watch::Receiver<StageTimings>,
    log_bus: LogBus,
) -> crate::Result<()> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;

    let setting_engine = crate::webrtc_settings::build_setting_engine();

    let api = Arc::new(
        APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .with_setting_engine(setting_engine)
            .build(),
    );

    // One shared, persistent H.264 track fed by whichever session is running.
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "wado".to_owned(),
    ));

    // How long frames wait between the compositor handing them over and this pump taking
    // them. Smoothed, because the client polls at ~1 Hz and a single raw sample of a 60 Hz
    // signal is meaningless noise.
    let queue_us = Arc::new(AtomicU64::new(0));

    pump::spawn(frame_rx, Arc::clone(&track), Arc::clone(&queue_us));

    let ctx = Arc::new(ServerCtx {
        api,
        track,
        cmd_tx,
        input_tx,
        log_bus,
        active_pc: Arc::new(Mutex::new(None)),
        last_request: Arc::new(AtomicU64::new(now_ms())),
        session_started: Arc::new(AtomicBool::new(false)),
        generation: Arc::new(AtomicU64::new(0)),
        timings,
        queue_us,
    });

    let listener = TcpListener::bind(&addr).await?;
    info!(%addr, "control server listening — connect with the wado-client app");

    tokio::spawn(watchdog::viewer_watchdog(
        ctx.cmd_tx.clone(),
        Arc::clone(&ctx.last_request),
        Arc::clone(&ctx.session_started),
        Arc::clone(&ctx.active_pc),
    ));

    loop {
        // `?` here was not a per-connection error: it exited `run_server` and left the
        // process alive with a running compositor and no control plane — no way to stop the
        // session, no way to start another. EMFILE and ECONNABORTED are transient and
        // reachable from outside (an SSE client holds a socket indefinitely, and there is no
        // connection cap), so shed the one connection and keep serving.
        let (stream, _peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!("accept failed ({e}) — continuing to serve");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        let ctx = Arc::clone(&ctx);
        tokio::spawn(async move {
            if let Err(e) = routes::handle_conn(stream, ctx).await {
                warn!("connection error: {e}");
            }
        });
    }
}
