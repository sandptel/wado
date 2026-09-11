//! wado-relay client — registers this server with the relay broker under its
//! Remote ID and handles all session + WebRTC signaling forwarded from
//! connected clients.
//!
//! When `WADO_RELAY_URL` is set (e.g. `ws://my-relay:4000`), `start` is called
//! instead of [`crate::website::start`]. The server is then only reachable through
//! the relay — no HTTP port needs to be open. The Remote ID (see
//! [`crate::remote_id`]) is the single token a client needs to connect.
//!
//! ## Threading
//! Same pattern as `website`: a dedicated std thread runs its own tokio runtime.
//! The relay WS runs inside that runtime. All compositor interaction goes through
//! the typed channel boundary (cmd_tx / input_tx / frame_rx).
//!
//! ## Resilience
//! The WebRTC API/track/frame-pump are built once; the relay WS connection is
//! wrapped in an **auto-reconnect loop** with exponential backoff (1 s → 30 s cap,
//! reset after a successful registration). A relay restart therefore only causes
//! a short re-registration gap, not a dead server.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tracing::{error, info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MediaEngine};
use webrtc::api::{API, APIBuilder};
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtcp::payload_feedbacks::full_intra_request::FullIntraRequest;
use webrtc::rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use wado_compositor::{CommandSender, CompositorCommand, FrameMsg, InputEvent, InputSender};
use wado_protocol::{INPUT_CHANNEL, relay::RelayMsg, relay::display_remote_id};

use crate::website::logbus::LogBus;

/// Reconnect backoff bounds.
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Everything a relay connection needs; lives across reconnects.
struct RelayCtx {
    api: Arc<API>,
    track: Arc<TrackLocalStaticSample>,
    cmd_tx: CommandSender,
    input_tx: InputSender,
    log_bus: LogBus,
    /// The single active viewer's peer connection (generation-guarded).
    active_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
    /// Bumped on every accepted offer; lets a stale viewer's teardown be ignored.
    generation: Arc<AtomicU64>,
    relay_url: String,
    remote_id: String,
    /// Latest per-stage render timings, answering `TimingRequest`. Latest-value-wins, so a
    /// slow or absent reader never backs the compositor up.
    timings: tokio::sync::watch::Receiver<wado_protocol::StageTimings>,
}

/// Spawn the relay client on a dedicated thread. Mirrors `website::start`.
pub fn start(
    cmd_tx: CommandSender,
    input_tx: InputSender,
    timings: tokio::sync::watch::Receiver<wado_protocol::StageTimings>,
    frame_rx: mpsc::Receiver<FrameMsg>,
    relay_url: String,
    remote_id: String,
    log_bus: LogBus,
) -> crate::Result<()> {
    std::thread::Builder::new()
        .name("wado-relay-client".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => { error!("relay client: failed to build tokio runtime: {e}"); return; }
            };
            rt.block_on(async move {
                if let Err(e) =
                    run(cmd_tx, input_tx, timings, frame_rx, relay_url, remote_id, log_bus).await
                {
                    error!("relay client exited with error: {e}");
                }
            });
        })?;
    Ok(())
}

async fn run(
    cmd_tx: CommandSender,
    input_tx: InputSender,
    timings: tokio::sync::watch::Receiver<wado_protocol::StageTimings>,
    mut frame_rx: mpsc::Receiver<FrameMsg>,
    relay_url: String,
    remote_id: String,
    log_bus: LogBus,
) -> crate::Result<()> {
    // ── Built once, survive reconnects ───────────────────────────────────────
    let api = Arc::new(build_webrtc_api()?);

    let track = Arc::new(TrackLocalStaticSample::new(
        webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "wado".to_owned(),
    ));
    {
        let track_pump = Arc::clone(&track);
        tokio::spawn(async move {
            // The frame channel is two slots deep, so a write_sample that takes longer than
            // two frame times is enough to start dropping. Whether it does is the difference
            // between "the link is saturated" and "the pump is the bottleneck", and nothing
            // else in the log distinguishes them.
            let mut slow: u64 = 0;
            let mut worst = Duration::ZERO;
            let mut worst_wait = Duration::ZERO;
            let mut queued_total = Duration::ZERO;
            let mut frames: u64 = 0;
            // Keyframe size, reported per stretch rather than only when something stalls.
            // The VBV cap is a ceiling on exactly this number, so it is the measurement that
            // says whether the cap is doing what it was set to do — and an IDR crushed against
            // it is visible as a once-per-GOP pulse of blockiness, which no timing metric
            // shows at all.
            let mut key_bytes_max: usize = 0;
            let mut key_bytes_total: usize = 0;
            let mut key_count: u64 = 0;
            let mut p_bytes_total: usize = 0;
            while let Some(frame) = frame_rx.recv().await {
                let bytes = frame.data.len();
                let key = is_keyframe(&frame.data);
                // How long the frame sat between the compositor letting go and the pump
                // picking it up. Relay mode has no /timing endpoint, so this leg — the one
                // the compositor explicitly cannot see — was measured and then thrown away.
                // It is also the leg that grows first when the pump falls behind, which
                // makes it an early warning rather than a post-mortem.
                let waited = frame.queued_at.elapsed();
                if waited > worst_wait {
                    worst_wait = waited;
                }
                queued_total += waited;
                frames += 1;
                if key {
                    key_count += 1;
                    key_bytes_total += bytes;
                    key_bytes_max = key_bytes_max.max(bytes);
                } else {
                    p_bytes_total += bytes;
                }
                if frames % 300 == 0 {
                    let p_frames = frames - key_count;
                    info!(
                        avg_queue_ms = (queued_total.as_millis() as u64) / frames.max(1),
                        worst_queue_ms = worst_wait.as_millis() as u64,
                        frames,
                        keyframes = key_count,
                        key_avg_kb = (key_bytes_total / key_count.max(1) as usize) / 1024,
                        key_max_kb = key_bytes_max / 1024,
                        p_avg_kb = (p_bytes_total / p_frames.max(1) as usize) / 1024,
                        "pump: queue wait over the last stretch"
                    );
                    queued_total = Duration::ZERO;
                    worst_wait = Duration::ZERO;
                    frames = 0;
                    key_count = 0;
                    key_bytes_total = 0;
                    key_bytes_max = 0;
                    p_bytes_total = 0;
                }
                // Relay mode has no /timing endpoint yet, so the queue stamp is unused
                // here — the duration still matters (real elapsed time keeps the RTP clock
                // on wall clock; see ChannelSink).
                let sample = Sample {
                    data: Bytes::from(frame.data),
                    duration: frame.duration,
                    ..Default::default()
                };
                let t0 = std::time::Instant::now();
                let runq0 = crate::sched::run_delay_ns();
                if let Err(e) = track_pump.write_sample(&sample).await {
                    warn!("relay client: write_sample: {e}");
                }
                let took = t0.elapsed();
                let runq = crate::sched::run_delay_ns().saturating_sub(runq0);
                let budget = frame.duration;
                if took > budget {
                    slow += 1;
                    if took > worst { worst = took; }
                    // Same once-per-60 cadence as the drop counter, so the two lines pair up.
                    if slow % 60 == 0 {
                        warn!(
                            slow,
                            worst_ms = worst.as_millis() as u64,
                            last_ms = took.as_millis() as u64,
                            budget_ms = budget.as_millis() as u64,
                            bytes,
                            keyframe = key,
                            "write_sample slower than the frame budget — pump is the bottleneck"
                        );
                    }
                    // Every overrun past a tenth of a second, with the three numbers that
                    // separate its possible causes.
                    //
                    // Size and packet count were the original suspects and have since been
                    // ruled out by their own data: 83 packets took 452 ms while 12 took
                    // 442 ms, so the stall is a roughly constant blocking event per frame
                    // rather than accumulated per-packet send cost. They are still logged,
                    // because that is the correlation that must keep failing to appear.
                    //
                    // `runq_ms` is what actually discriminates. If it approaches `took_ms`
                    // the thread was runnable and starved of CPU, and giving the session's
                    // applications a smaller CPU share is a real fix. If it is near zero the
                    // thread was blocked on something else and no amount of priority tuning
                    // will touch this — in which case `psi_mem` is the next suspect, since
                    // reclaim driven by another process stalls a thread for hundreds of
                    // milliseconds while every CPU metric says the machine is idle.
                    if took > Duration::from_millis(100) {
                        let packets = (bytes / MTU_PAYLOAD).max(1);
                        warn!(
                            took_ms = took.as_millis() as u64,
                            runq_ms = runq / 1_000_000,
                            psi_cpu = crate::sched::pressure_some_avg10("/proc/pressure/cpu"),
                            psi_mem = crate::sched::pressure_some_avg10("/proc/pressure/memory"),
                            bytes,
                            keyframe = key,
                            packets,
                            "write_sample stall"
                        );
                    }
                }
            }
        });
    }

    let ctx = RelayCtx {
        api,
        track,
        cmd_tx,
        input_tx,
        log_bus,
        active_pc: Arc::new(Mutex::new(None)),
        generation: Arc::new(AtomicU64::new(0)),
        relay_url,
        remote_id,
        timings,
    };

    // ── Reconnect loop ──────────────────────────────────────────────────────
    let mut backoff = BACKOFF_INITIAL;
    let mut first_attempt = true;
    loop {
        match connect_and_serve(&ctx).await {
            // Ok = we registered successfully and the connection later closed:
            // the relay restarted or the network blipped. Retry promptly.
            Ok(()) => {
                info!("relay client: connection to relay lost — reconnecting in {backoff:?}");
                backoff = BACKOFF_INITIAL;
            }
            // Err = we never got as far as a successful registration.
            Err(e) => {
                if first_attempt {
                    warn!("relay client: cannot reach relay ({e}) — retrying in {backoff:?}");
                } else {
                    info!("relay client: relay still unreachable ({e}) — retrying in {backoff:?}");
                }
            }
        }
        first_attempt = false;
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// One relay connection: WS connect → Register → serve messages until the
/// connection drops. `Ok(())` means registration succeeded (connection ended
/// later); `Err` means we never registered.
async fn connect_and_serve(ctx: &RelayCtx) -> crate::Result<()> {
    // ── 1. Connect ───────────────────────────────────────────────────────────
    let register_url = format!("{}/register", ctx.relay_url);
    let (ws_stream, _) = connect_async(&register_url)
        .await
        .map_err(|e| crate::WadoError::Other(format!("connect: {e}")))?;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // All outbound relay WS messages go through this channel so multiple tasks
    // can write without fighting over the SplitSink.
    let (out_tx, mut out_rx) = mpsc::channel::<String>(128);
    let write_task = tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if ws_sink.send(WsMsg::Text(text)).await.is_err() {
                break;
            }
        }
    });

    // ── 2. Register ──────────────────────────────────────────────────────────
    let display_name = std::env::var("HOSTNAME").ok();
    send_relay(&out_tx, &RelayMsg::Register {
        remote_id: ctx.remote_id.clone(),
        display_name,
    }).await?;

    match ws_stream.next().await {
        Some(Ok(WsMsg::Text(t))) => match serde_json::from_str::<RelayMsg>(&t) {
            Ok(RelayMsg::Registered { remote_id }) => {
                info!("relay client: registered — Remote ID {}", display_remote_id(&remote_id));
            }
            Ok(RelayMsg::Error { message }) => {
                write_task.abort();
                return Err(crate::WadoError::Other(format!("relay rejected registration: {message}")));
            }
            Ok(other) => {
                write_task.abort();
                return Err(crate::WadoError::Other(format!("unexpected relay response: {other:?}")));
            }
            Err(e) => {
                write_task.abort();
                return Err(crate::WadoError::Other(format!("bad relay response JSON: {e}")));
            }
        },
        _ => {
            write_task.abort();
            return Err(crate::WadoError::Other("relay closed before Registered".into()));
        }
    }

    // ── 3. Log forwarding task (per connection) ──────────────────────────────
    let log_task = {
        let mut log_rx: broadcast::Receiver<String> = ctx.log_bus.subscribe();
        let out_tx_log = out_tx.clone();
        tokio::spawn(async move {
            loop {
                match log_rx.recv().await {
                    Ok(line) => {
                        let _ = send_relay_nowait(&out_tx_log, &RelayMsg::Log { line }).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };

    info!("relay client: ready — clients can connect with the Remote ID");

    // The viewer's interactive shell, if they have opened one. A local rather than
    // connection state held elsewhere, so that losing the connection drops it — and
    // dropping a `Pty` kills the shell and, through SIGHUP, whatever it was running.
    let mut pty: Option<crate::pty::Pty> = None;

    // ── 4. Main message loop ─────────────────────────────────────────────────
    while let Some(frame) = ws_stream.next().await {
        let text = match frame {
            Ok(WsMsg::Text(t)) => t,
            Ok(WsMsg::Ping(_)) | Ok(WsMsg::Pong(_)) => continue,
            Ok(WsMsg::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => { warn!("relay client: WS error: {e}"); break; }
        };

        let msg = match serde_json::from_str::<RelayMsg>(&text) {
            Ok(m) => m,
            Err(e) => { warn!("relay client: bad JSON from relay: {e}"); continue; }
        };

        match msg {
            RelayMsg::PeerConnected { room_id, client_addr } => {
                info!(room_id = %room_id, client = %client_addr, "relay client: peer connected");
            }

            RelayMsg::SessionStart { config } => {
                let (reply_tx, reply_rx) = oneshot::channel();
                if ctx.cmd_tx.send(CompositorCommand::Start { config, reply: reply_tx }).is_err() {
                    send_relay(&out_tx, &RelayMsg::SessionError {
                        message: "compositor unavailable".into(),
                    }).await.ok();
                    continue;
                }
                match tokio::time::timeout(Duration::from_secs(5), reply_rx).await {
                    Ok(Ok(Ok(info))) => {
                        send_relay(&out_tx, &RelayMsg::SessionStarted { info }).await.ok();
                    }
                    Ok(Ok(Err(msg))) => {
                        send_relay(&out_tx, &RelayMsg::SessionError { message: msg }).await.ok();
                    }
                    Ok(Err(_)) => {
                        send_relay(&out_tx, &RelayMsg::SessionError {
                            message: "compositor dropped reply".into(),
                        }).await.ok();
                    }
                    Err(_) => {
                        send_relay(&out_tx, &RelayMsg::SessionError {
                            message: "session start timed out".into(),
                        }).await.ok();
                    }
                }
            }

            RelayMsg::SessionStop => {
                let _ = ctx.cmd_tx.send(CompositorCommand::Stop);
                send_relay(&out_tx, &RelayMsg::SessionStopped).await.ok();
            }

            RelayMsg::SessionLaunch { command } => {
                let _ = ctx.cmd_tx.send(CompositorCommand::Launch { command });
                send_relay(&out_tx, &RelayMsg::SessionLaunched).await.ok();
            }

            RelayMsg::AppsRequest => {
                let apps = crate::apps::discover();
                send_relay(&out_tx, &RelayMsg::AppsList { apps }).await.ok();
            }

            RelayMsg::Exec { command } => {
                // Spawned rather than awaited: a command that never exits must not stop the
                // relay loop from carrying input, frames or session control. The viewer's
                // channel is cloned in, so output flows for as long as the socket lives.
                let out_tx = out_tx.clone();
                tokio::spawn(async move {
                    info!(command, "exec");
                    let (tx, mut rx) = mpsc::channel::<crate::exec::Line>(256);
                    let forward = {
                        let out_tx = out_tx.clone();
                        tokio::spawn(async move {
                            while let Some(l) = rx.recv().await {
                                if send_relay(
                                    &out_tx,
                                    &RelayMsg::ExecOutput { line: l.text, err: l.err },
                                )
                                .await
                                .is_err()
                                {
                                    break;
                                }
                            }
                        })
                    };
                    let code = match crate::exec::run(&command, tx).await {
                        Ok(code) => code,
                        Err(e) => {
                            send_relay(&out_tx, &RelayMsg::ExecOutput {
                                line: format!("wado: {e}"),
                                err: true,
                            })
                            .await
                            .ok();
                            Some(127)
                        }
                    };
                    let _ = forward.await;
                    send_relay(&out_tx, &RelayMsg::ExecExit { code }).await.ok();
                });
            }

            RelayMsg::PtyOpen { cols, rows } => {
                // Replaces any existing shell: one per viewer. The old one is dropped first
                // so its shell is gone before the new one starts, rather than both running.
                pty = None;
                let (tx, mut rx) = mpsc::channel::<String>(64);
                match crate::pty::Pty::open(cols, rows, tx) {
                    Ok(p) => {
                        pty = Some(p);
                        let out_tx = out_tx.clone();
                        tokio::spawn(async move {
                            while let Some(data) = rx.recv().await {
                                if send_relay(&out_tx, &RelayMsg::PtyOutput { data }).await.is_err()
                                {
                                    break;
                                }
                            }
                            // The channel closing means the reader thread saw EOF, which is
                            // how a shell exiting looks from here.
                            //
                            // ponytail: no exit code. The thread that notices the exit is the
                            // one reading the master and it does not hold the child handle;
                            // plumbing the code back is a channel this does not need to show
                            // "[shell exited]".
                            let _ = send_relay(&out_tx, &RelayMsg::PtyExit { code: None }).await;
                        });
                    }
                    Err(e) => {
                        warn!("pty open failed: {e}");
                        send_relay(&out_tx, &RelayMsg::PtyOutput {
                            data: format!("wado: could not start a shell: {e}\r\n"),
                        })
                        .await
                        .ok();
                        send_relay(&out_tx, &RelayMsg::PtyExit { code: None }).await.ok();
                    }
                }
            }

            RelayMsg::PtyInput { data } => {
                if let Some(p) = pty.as_mut() {
                    if let Err(e) = p.write(&data) {
                        warn!("pty write failed: {e}");
                        pty = None;
                    }
                }
            }

            RelayMsg::PtyResize { cols, rows } => {
                if let Some(p) = pty.as_mut() {
                    p.resize(cols, rows);
                }
            }

            RelayMsg::PtyClose => {
                // Drop kills the shell; SIGHUP takes its jobs with it.
                pty = None;
            }

            RelayMsg::TimingRequest => {
                let timings = *ctx.timings.borrow();
                send_relay(&out_tx, &RelayMsg::Timing { timings }).await.ok();
            }

            RelayMsg::SessionWindow { action } => {
                let _ = ctx.cmd_tx.send(CompositorCommand::Window(action));
                send_relay(&out_tx, &RelayMsg::SessionWindowed).await.ok();
            }

            RelayMsg::SdpOffer { sdp } => {
                match handle_sdp_offer(ctx, sdp, out_tx.clone()).await {
                    Ok(()) => {}
                    Err(e) => {
                        error!("relay client: SdpOffer handling failed: {e}");
                        send_relay(&out_tx, &RelayMsg::SessionError {
                            message: format!("WebRTC setup failed: {e}"),
                        }).await.ok();
                    }
                }
            }

            RelayMsg::IceCandidate { candidate } => {
                let pc = ctx.active_pc.lock().unwrap().clone();
                if let Some(pc) = pc {
                    if let Ok(cand) = serde_json::from_str(&candidate) {
                        let _ = pc.add_ice_candidate(cand).await;
                    }
                }
            }

            RelayMsg::ClientLog { line } => {
                info!("browser: {line}");
            }

            RelayMsg::Ping => { send_relay(&out_tx, &RelayMsg::Pong).await.ok(); }

            other => {
                warn!("relay client: unexpected message: {other:?}");
            }
        }
    }

    log_task.abort();
    write_task.abort();
    Ok(())
}

/// Create a new RTCPeerConnection, attach the shared track, wire input data
/// channel, RTCP PLI → ForceKeyframe, state-change → Stop, gather ICE, answer.
///
/// Bytes of payload per RTP packet, near enough. Only used to turn a frame size into a packet
/// count for the stall trace, so the usual 1200-byte MTU budget is close enough to be useful
/// and exactness would not change what the number says.
const MTU_PAYLOAD: usize = 1200;

/// Whether an Annex-B access unit contains an IDR slice (NAL type 5).
///
/// Only the first few NAL headers are examined: SPS/PPS are prepended to IDR frames, so the
/// slice that decides this is near the front, and scanning a 200 KB buffer per frame in the
/// pump would be its own bottleneck.
fn is_keyframe(data: &[u8]) -> bool {
    let mut seen = 0;
    let mut i = 0;
    while i + 4 < data.len() && seen < 8 {
        // Annex-B start code: 00 00 01 or 00 00 00 01.
        let (hdr, next) = if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            (data[i + 3], i + 4)
        } else if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0 && data[i + 3] == 1 {
            (data[i + 4.min(data.len() - 1)], i + 5)
        } else {
            i += 1;
            continue;
        };
        if hdr & 0x1f == 5 {
            return true;
        }
        seen += 1;
        i = next;
    }
    false
}

/// Summarise the ICE candidate types present in an SDP — "host", "srflx", "relay".
fn candidate_types(sdp: &str) -> String {
    let mut types: Vec<&str> = sdp
        .lines()
        .filter(|l| l.starts_with("a=candidate:"))
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            // a=candidate:<foundation> <component> <proto> <priority> <ip> <port> typ <type>
            parts.position(|w| w == "typ").and_then(|_| parts.next())
        })
        .collect();
    types.sort_unstable();
    types.dedup();
    if types.is_empty() { "none".to_string() } else { types.join(",") }
}

async fn handle_sdp_offer(
    ctx: &RelayCtx,
    offer_json: String,
    out_tx: mpsc::Sender<String>,
) -> crate::Result<()> {
    let offer: RTCSessionDescription = serde_json::from_str(&offer_json)
        .map_err(|e| crate::WadoError::Other(format!("bad SDP: {e}")))?;

    // The offer carries every candidate the browser gathered (non-trickle). Their types are
    // the whole diagnosis when media never flows: host-only means STUN was blocked and no
    // route past NAT was ever found, srflx present means the path failed somewhere later.
    info!(
        "relay client: offer received — {} candidates ({})",
        offer.sdp.matches("a=candidate:").count(),
        candidate_types(&offer.sdp)
    );

    let pc = Arc::new(
        ctx.api
            .new_peer_connection(RTCConfiguration {
                ice_servers: crate::ice::servers(),
                ..Default::default()
            })
            .await?,
    );

    let rtp_sender = pc
        .add_track(Arc::clone(&ctx.track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // Input data channel: forward JSON InputEvents to the compositor.
    {
        let input_tx = ctx.input_tx.clone();
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let input_tx = input_tx.clone();
            Box::pin(async move {
                if dc.label() != INPUT_CHANNEL { return; }
                dc.on_open(Box::new(|| Box::pin(async { info!("relay client: input data channel open") })));
                dc.on_message(Box::new(move |msg: DataChannelMessage| {
                    let input_tx = input_tx.clone();
                    Box::pin(async move {
                        match serde_json::from_slice::<InputEvent>(&msg.data) {
                            Ok(ev) => { let _ = input_tx.send(ev); }
                            Err(e) => warn!("relay client: bad input event: {e}"),
                        }
                    })
                }));
            })
        }));
    }

    // Generation guard: only the current viewer's teardown stops the session.
    let my_gen = ctx.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *ctx.active_pc.lock().unwrap() = Some(Arc::clone(&pc));

    // RTCP read loop: PLI/FIR → ForceKeyframe.
    {
        let cmd_tx = ctx.cmd_tx.clone();
        tokio::spawn(async move {
            loop {
                match rtp_sender.read_rtcp().await {
                    Ok((packets, _)) => {
                        for p in packets {
                            let a = p.as_any();
                            if a.downcast_ref::<PictureLossIndication>().is_some()
                                || a.downcast_ref::<FullIntraRequest>().is_some()
                            {
                                let _ = cmd_tx.send(CompositorCommand::ForceKeyframe);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Connection state change.
    {
        let cmd_tx = ctx.cmd_tx.clone();
        let generation = Arc::clone(&ctx.generation);
        pc.on_peer_connection_state_change(Box::new(move |state| {
            match state {
                RTCPeerConnectionState::Connected => {
                    info!("relay client: viewer connected via WebRTC");
                    let _ = cmd_tx.send(CompositorCommand::ForceKeyframe);
                }
                RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                    if generation.load(Ordering::SeqCst) == my_gen {
                        info!("relay client: viewer gone — stopping session");
                        let _ = cmd_tx.send(CompositorCommand::Stop);
                    }
                }
                other => info!(?other, "relay client: peer connection state"),
            }
            Box::pin(async {})
        }));
    }

    // ICE-level transitions. The peer-connection state hides where a failure happened;
    // this is the one that distinguishes "never got a reply" (stuck Checking) from
    // "candidates exhausted" (Failed).
    pc.on_ice_connection_state_change(Box::new(|state| {
        info!(?state, "relay client: ICE connection state");
        Box::pin(async {})
    }));

    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    // Bounded: see `crate::ice::GATHER_WAIT`. Waiting for gathering to *complete* is what a
    // dead STUN server turns into a 15-second answer, and the client has left by then.
    if tokio::time::timeout(crate::ice::GATHER_WAIT, gather.recv())
        .await
        .is_err()
    {
        tracing::warn!(
            "ICE gathering still running after {:?} — answering with what we have. A STUN \
             server is not replying; webrtc-ice gives each one a fixed 5s before giving up.",
            crate::ice::GATHER_WAIT
        );
    }

    let local = pc
        .local_description()
        .await
        .ok_or_else(|| crate::WadoError::Other("no local description after ICE gather".into()))?;
    let answer_json = serde_json::to_string(&local)?;

    info!(
        "relay client: answer sent — {} candidates ({})",
        local.sdp.matches("a=candidate:").count(),
        candidate_types(&local.sdp)
    );
    // Said loudly because the alternative is watching ICE fail and guessing. Host-only is not
    // a weaker connection, it is one that cannot be made from outside this LAN.
    if !crate::ice::has_reflexive(&local.sdp) {
        tracing::warn!(
            "no server-reflexive candidate — every STUN server timed out, so this answer only \
             works on the local network. A client elsewhere hangs on 'starting session' while \
             ICE goes checking, disconnected, failed."
        );
    }

    send_relay(&out_tx, &RelayMsg::SdpAnswer { sdp: answer_json }).await?;
    Ok(())
}

fn build_webrtc_api() -> crate::Result<API> {
    let mut media = MediaEngine::default();
    media.register_default_codecs()?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media)?;

    let settings = crate::webrtc_settings::build_setting_engine();

    Ok(APIBuilder::new()
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .with_setting_engine(settings)
        .build())
}

async fn send_relay(tx: &mpsc::Sender<String>, msg: &RelayMsg) -> crate::Result<()> {
    let text = serde_json::to_string(msg)?;
    tx.send(text).await.map_err(|_| crate::WadoError::Other("relay out channel closed".into()))
}

/// Non-fallible version for fire-and-forget (log lines, pongs).
async fn send_relay_nowait(tx: &mpsc::Sender<String>, msg: &RelayMsg) -> Option<()> {
    let text = serde_json::to_string(msg).ok()?;
    tx.try_send(text).ok()
}

#[cfg(test)]
mod pump_tests {
    use super::is_keyframe;

    #[test]
    fn finds_an_idr_behind_sps_and_pps() {
        // SPS (7), PPS (8), then the IDR slice (5) — the order a real encoder emits.
        let au = [
            0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x68, 0xce, 0, 0, 0, 1, 0x65, 0x88, 0x84,
        ];
        assert!(is_keyframe(&au));
    }

    #[test]
    fn a_p_frame_is_not_a_keyframe() {
        let au = [0, 0, 0, 1, 0x41, 0x9a, 0x12, 0x34, 0x56, 0x78];
        assert!(!is_keyframe(&au));
    }

    #[test]
    fn short_and_empty_buffers_do_not_panic() {
        for au in [&[][..], &[0][..], &[0, 0, 1][..], &[0, 0, 0, 1][..]] {
            let _ = is_keyframe(au);
        }
    }
}
