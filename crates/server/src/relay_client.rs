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

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{FutureExt, SinkExt, StreamExt};
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
    /// Whether the focused application wants text input (`zwp_text_input_v3`). Forwarded to the
    /// viewer so a phone raises its soft keyboard by itself. Latest-value-wins for the same
    /// reason as `timings`, and because it is state: a viewer attaching mid-edit must be told
    /// the current answer, not left waiting for the next change.
    text_input: tokio::sync::watch::Receiver<bool>,
    /// The render-tick divisor in force (`compositor::congestion`). Forwarded so the viewer can
    /// tell a frame rate *it asked us to reduce* from a compositor that has stopped producing.
    /// Latest-value-wins and marked changed on attach, for the same reasons as `text_input`.
    shedding: tokio::sync::watch::Receiver<u32>,
    /// Bitrate actually written to the video track over the last stretch, in kbps. Forwarded to
    /// the viewer — see [`wado_protocol::RelayMsg::SentKbps`] for why it has to be.
    sent_kbps: tokio::sync::watch::Receiver<u32>,
    /// Unix-millis of the last moment a viewer's peer connection was seen `Connected`. See
    /// [`viewer_watchdog`] — this, not relay silence, is how long there has been no viewer.
    last_connected: Arc<AtomicU64>,
    /// Unix-millis of the last message received from the relay. See [`viewer_watchdog`].
    last_relay_msg: Arc<AtomicU64>,
    /// Whether a session was started for a viewer and has not been stopped. See
    /// [`viewer_watchdog`].
    session_started: Arc<AtomicBool>,
}

/// Now, in unix milliseconds.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Spawn the relay client on a dedicated thread. Mirrors `website::start`.
pub fn start(
    cmd_tx: CommandSender,
    input_tx: InputSender,
    timings: tokio::sync::watch::Receiver<wado_protocol::StageTimings>,
    text_input: tokio::sync::watch::Receiver<bool>,
    shedding: tokio::sync::watch::Receiver<u32>,
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
                    run(cmd_tx, input_tx, timings, text_input, shedding, frame_rx, relay_url, remote_id,
                        log_bus)
                    .await
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
    text_input: tokio::sync::watch::Receiver<bool>,
    shedding: tokio::sync::watch::Receiver<u32>,
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
    // What actually leaves this process, measured at the track. Published for the viewer, which
    // otherwise sees only what arrived and must guess which end lost the difference.
    let (sent_kbps_tx, sent_kbps) = tokio::sync::watch::channel(0u32);
    {
        let track_pump = Arc::clone(&track);
        tokio::spawn(async move {
            // The frame channel is two slots deep, so a write_sample that takes longer than
            // two frame times is enough to start dropping. Whether it does is the difference
            // between "the link is saturated" and "the pump is the bottleneck", and nothing
            // else in the log distinguishes them.
            let mut slow: u64 = 0;
            let mut pump = crate::pumpstats::PumpStats::new();
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
            // Bytes actually handed to the track, and the wall clock they took. This is the one
            // number that separates "the sender stopped" from "the path ate it", and only this
            // process has it — see `RelayMsg::SentKbps`.
            let mut stretch_bytes: usize = 0;
            let mut stretch_start = Instant::now();
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
                stretch_bytes += bytes;
                if key {
                    key_count += 1;
                    key_bytes_total += bytes;
                    key_bytes_max = key_bytes_max.max(bytes);
                } else {
                    p_bytes_total += bytes;
                }
                if frames % 300 == 0 {
                    // Tell the viewer what actually left here. Without it the viewer sees only
                    // what arrived and has to guess which end lost the difference — and it has
                    // guessed wrong twice, measured: 2026-09-12 22:33 (2.4 Mbps of 5.35 sent) and
                    // 2026-09-13 01:19 (524 kbps of 5.13 sent), both reported as `bad the server`
                    // with `lost=0` while the render loop held 90/90 fps.
                    let secs = stretch_start.elapsed().as_secs_f64();
                    if secs > 0.0 {
                        let kbps = ((stretch_bytes as f64 * 8.0) / secs / 1000.0) as u32;
                        // A watch channel, not the relay socket: this task is spawned once and
                        // outlives every relay connection, so it cannot hold a sender that a
                        // reconnect replaces.
                        let _ = sent_kbps_tx.send(kbps);
                    }
                    stretch_bytes = 0;
                    stretch_start = Instant::now();

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
                // EVERY frame, not just the slow ones. The `> 100 ms` warning below reports
                // outliers and cannot report their context: with only outliers logged there is
                // no way to tell a pump that is healthy-with-rare-spikes from one that is
                // chronically late, and `memory/latency/07` records a pattern that was read off
                // that censored view and turned out not to exist. Percentiles come from the
                // whole distribution or they are not percentiles.
                pump.record(took);
                if let Some(line) = pump.due() {
                    tracing::info!(
                        frames = line.n,
                        p50_ms = format!("{:.1}", line.p50),
                        p90_ms = format!("{:.1}", line.p90),
                        p99_ms = format!("{:.1}", line.p99),
                        max_ms = format!("{:.1}", line.max),
                        over_budget = line.over_budget,
                        budget_ms = budget.as_millis() as u64,
                        "write_sample distribution over the last stretch"
                    );
                }
                if took > budget {
                    pump.record_over_budget();
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
        text_input,
        shedding,
        sent_kbps,
        active_pc: Arc::new(Mutex::new(None)),
        generation: Arc::new(AtomicU64::new(0)),
        last_connected: Arc::new(AtomicU64::new(now_ms())),
        last_relay_msg: Arc::new(AtomicU64::new(now_ms())),
        session_started: Arc::new(AtomicBool::new(false)),
        relay_url,
        remote_id,
        timings,
    };

    tokio::spawn(viewer_watchdog(
        ctx.cmd_tx.clone(),
        Arc::clone(&ctx.last_connected),
        Arc::clone(&ctx.last_relay_msg),
        Arc::clone(&ctx.session_started),
        Arc::clone(&ctx.active_pc),
    ));

    // ── Reconnect loop ──────────────────────────────────────────────────────
    let mut backoff = BACKOFF_INITIAL;
    let mut first_attempt = true;
    loop {
        // `catch_unwind` around the whole connection, not just around a suspicious line.
        //
        // Without it a panic anywhere in `connect_and_serve` — a per-message handler, a
        // poisoned lock — unwinds through `run`, through `block_on`, and kills the
        // `wado-relay-client` thread for good. Nothing restarts it. The process stays up, the
        // compositor keeps running, `/proc` looks healthy, and **no client can ever reach this
        // server again**: the exact signature of the relay panic on 2026-09-12, one layer down.
        // Treated as a lost connection, which is what the reconnect loop below already knows
        // how to survive.
        let attempt = AssertUnwindSafe(connect_and_serve(&ctx)).catch_unwind().await;
        let attempt = match attempt {
            Ok(r) => r,
            Err(_) => {
                error!("relay client: panicked serving a connection — reconnecting");
                Ok(())
            }
        };
        match attempt {
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

    // ── 3b. Text-input forwarding task (per connection) ──────────────────────
    //
    // State, not an event: the current value is sent immediately on attach (watch channels
    // deliver the held value to a fresh subscriber), so a viewer joining a session that is
    // already sitting in a text field gets its keyboard up without waiting for the next change.
    let text_input_task = {
        let mut rx = ctx.text_input.clone();
        let out_tx_ti = out_tx.clone();
        tokio::spawn(async move {
            // Mark the held value unseen so the first iteration sends it rather than waiting.
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let active = *rx.borrow_and_update();
                let _ = send_relay_nowait(&out_tx_ti, &RelayMsg::TextInput { active }).await;
            }
        })
    };

    // Same shape and same reason as `text_input_task` above: latest-value-wins state, marked
    // changed so a viewer attaching to an already-shedding session is told the divisor rather
    // than left to assume 1 and blame the sender for the frame rate.
    let shedding_task = {
        let mut rx = ctx.shedding.clone();
        let out_tx_sh = out_tx.clone();
        tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let divisor = *rx.borrow_and_update();
                let _ = send_relay_nowait(&out_tx_sh, &RelayMsg::Shedding { divisor }).await;
            }
        })
    };

    // Third of the same shape (see `text_input_task`, `shedding_task`): latest-value-wins state,
    // marked changed so a viewer attaching mid-session gets the current figure immediately.
    let sent_kbps_task = {
        let mut rx = ctx.sent_kbps.clone();
        let out_tx_sk = out_tx.clone();
        tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let kbps = *rx.borrow_and_update();
                if kbps > 0 {
                    let _ = send_relay_nowait(&out_tx_sk, &RelayMsg::SentKbps { kbps }).await;
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

        // Liveness for `viewer_watchdog`. Bumped on every frame, including ones we do not
        // understand — the point is that the relay link is carrying traffic, not what it says.
        ctx.last_relay_msg.store(now_ms(), Ordering::Relaxed);

        let msg = match serde_json::from_str::<RelayMsg>(&text) {
            Ok(m) => m,
            Err(e) => {
                // Answered, not just logged. A dropped message leaves the sender waiting out a
                // timeout with no way to tell "the daemon rejected this" from "the daemon is
                // wedged" — which cost a debugging cycle here on 2026-09-13, when a probe sent
                // the wrong shape for `Quality` and simply hung.
                warn!("relay client: bad JSON from relay: {e}");
                send_relay(&out_tx, &RelayMsg::SessionError {
                    message: format!("this daemon could not understand that message: {e}"),
                }).await.ok();
                continue;
            }
        };

        match msg {
            RelayMsg::PeerConnected { room_id, client_addr } => {
                info!(room_id = %room_id, client = %client_addr, "relay client: peer connected");
            }

            RelayMsg::PeerDisconnected { room_id } => {
                // Deliberately *not* a teardown. See `RelayMsg::PeerDisconnected`: the relay used
                // to synthesize a `SessionStop` here, which made every dropped socket cost the
                // viewer their windows and their applications. The session stays up and
                // `viewer_watchdog` is left to decide, which is the only place that decision
                // belongs — it is the one thing here with a clock.
                //
                // The peer connection *is* closed, because it is certainly dead and webrtc-rs
                // releases its ICE sockets only on `close().await`, never on `Drop` (I14). Left
                // open, each abandoned viewer holds four of the 101 pinned UDP ports until the
                // next offer happens to replace it.
                let _ = ctx.cmd_tx.send(CompositorCommand::ViewerAttached(false));
                let dead = ctx.active_pc.lock().unwrap_or_else(|e| e.into_inner()).take();
                if let Some(pc) = dead {
                    tokio::spawn(async move {
                        if let Err(e) = pc.close().await {
                            warn!("relay client: closing a departed viewer's peer connection: {e}");
                        }
                    });
                }
                info!(
                    room_id = %room_id,
                    "relay client: viewer disconnected — session kept, {VIEWER_GRACE:?} of grace \
                     starts from the last moment it was connected"
                );
            }

            RelayMsg::SessionStart { config } => {
                if let Err(why) = config.validate() {
                    warn!("relay client: refusing an invalid session config: {why}");
                    send_relay(&out_tx, &RelayMsg::SessionError {
                        message: format!("that configuration cannot work: {why}"),
                    }).await.ok();
                    continue;
                }
                // A session already running is not an error, it is a choice. Telling the viewer
                // "a session is already active" and closing the socket — which is what this did —
                // left a reconnecting phone with a session it could see in the logs and no way to
                // reach. See `RelayMsg::SessionAlive`.
                if let Some(info) = live_session(&ctx).await {
                    info!("relay client: a session is already running — offering rejoin or drop");
                    send_relay(&out_tx, &RelayMsg::SessionAlive { info }).await.ok();
                    continue;
                }
                ctx.session_started.store(true, Ordering::SeqCst);
                // The grace period starts now: a viewer that asked for a session but never
                // completes ICE must still be reaped, and it has never been connected.
                ctx.last_connected.store(now_ms(), Ordering::Relaxed);
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

            RelayMsg::SessionRejoin => {
                match live_session(&ctx).await {
                    Some(info) => {
                        ctx.session_started.store(true, Ordering::SeqCst);
                        ctx.last_connected.store(now_ms(), Ordering::Relaxed);
                        // The per-viewer reset (strain, shed divisor, congestion window) and the
                        // keyframe used to be sent from here by hand. They belong to
                        // `ViewerAttached`, which the peer-connection handler sends when this
                        // viewer's media path actually comes up — the rejoin message itself only
                        // means the button was pressed.
                        info!("relay client: viewer rejoined the running session");
                        send_relay(&out_tx, &RelayMsg::SessionStarted { info }).await.ok();
                    }
                    None => {
                        // The window between being offered the choice and taking it is real: the
                        // viewer watchdog stops idle sessions, so the thing being joined can be
                        // gone by the time the button is pressed.
                        send_relay(&out_tx, &RelayMsg::SessionError {
                            message: "the session ended before you could rejoin it".into(),
                        }).await.ok();
                    }
                }
            }

            RelayMsg::SessionReconfigure { config } => {
                // Same guard as `SessionStart`, from the same `SessionConfig::validate` — the
                // two verbs take the same struct from the same untrusted socket, and a check on
                // one of them is a check the other silently does not have.
                if let Err(why) = config.validate() {
                    warn!("relay client: refusing an invalid reconfigure: {why}");
                    send_relay(&out_tx, &RelayMsg::SessionError {
                        message: format!("that configuration cannot work: {why}"),
                    }).await.ok();
                    continue;
                }
                // No `session_started` reply and no renegotiation — see
                // `RelayMsg::SessionReconfigure`. The viewer keeps the peer connection it has;
                // the picture changes shape at the forced IDR.
                let (reply_tx, reply_rx) = oneshot::channel();
                if ctx.cmd_tx.send(CompositorCommand::Reconfigure { config, reply: reply_tx }).is_err() {
                    send_relay(&out_tx, &RelayMsg::SessionError {
                        message: "compositor unavailable".into(),
                    }).await.ok();
                    continue;
                }
                // The same bound every other compositor round-trip here uses: rebuilding an
                // encoder can fail slowly, and a wedged render loop must not hold the relay
                // socket waiting for an answer that is not coming.
                match tokio::time::timeout(Duration::from_secs(5), reply_rx).await {
                    Ok(Ok(Ok(info))) => {
                        send_relay(&out_tx, &RelayMsg::SessionReconfigured { info }).await.ok();
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
                            message: "reconfigure timed out".into(),
                        }).await.ok();
                    }
                }
            }

            RelayMsg::SessionStop => {
                ctx.session_started.store(false, Ordering::SeqCst);
                let _ = ctx.cmd_tx.send(CompositorCommand::Stop);
                send_relay(&out_tx, &RelayMsg::SessionStopped).await.ok();
            }

            RelayMsg::ViewerStrain { strained } => {
                // Straight through to the compositor. No rate limiting: the client sends this
                // only when its settled verdict changes, and the render loop reads it once per
                // decision window rather than per message.
                let _ = ctx.cmd_tx.send(CompositorCommand::ViewerStrain(strained));
            }

            RelayMsg::SessionLaunch { command } => {
                // Answered honestly. This used to send `SessionLaunched` unconditionally, so a
                // launch into a session that does not exist was reported as a success — the
                // compositor logged `launch ignored — no active session` and the viewer was told
                // the application was starting. It then waited for a window that was never
                // coming, with the only evidence in a log it cannot see.
                if live_session(&ctx).await.is_none() {
                    send_relay(&out_tx, &RelayMsg::SessionError {
                        message: "there is no session to launch into — start one first".into(),
                    }).await.ok();
                    continue;
                }
                let _ = ctx.cmd_tx.send(CompositorCommand::Launch { command });
                send_relay(&out_tx, &RelayMsg::SessionLaunched).await.ok();
            }

            RelayMsg::AppsRequest => {
                let apps = crate::apps::discover();
                send_relay(&out_tx, &RelayMsg::AppsList { apps }).await.ok();
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
                let pc = ctx.active_pc.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
    text_input_task.abort();
    shedding_task.abort();
    sent_kbps_task.abort();
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
    let previous = ctx
        .active_pc
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Arc::clone(&pc));

    // Dropping an `RTCPeerConnection` does NOT free it: webrtc-rs holds the ICE agent, its
    // gathering tasks and its UDP sockets behind internal `Arc`s, and only `close()` tears them
    // down. Replacing the handle without closing leaks a socket set per re-offer.
    //
    // Observed 2026-09-12: after a few hours of reconnect churn the daemon began answering with
    // `2 candidates (host)` and then `0 candidates (none)` — no srflx, eventually not even a host
    // candidate — and no client could connect until it was restarted. Sockets it could no longer
    // get. Re-offers are about to become the normal recovery path rather than a rare one, so this
    // has to be right before that change is worth anything.
    //
    // Spawned rather than awaited: `close()` waits on the agent's own tasks, and this is the
    // negotiation path — the new answer must not queue behind the old connection's shutdown.
    if let Some(old) = previous {
        tokio::spawn(async move {
            if let Err(e) = old.close().await {
                warn!("relay client: closing the previous peer connection: {e}");
            }
        });
    }

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
        let last_connected = Arc::clone(&ctx.last_connected);
        pc.on_peer_connection_state_change(Box::new(move |state| {
            match state {
                RTCPeerConnectionState::Connected => {
                    info!("relay client: viewer connected via WebRTC");
                    last_connected.store(now_ms(), Ordering::Relaxed);
                    // Resume rendering and reset the per-viewer state. The keyframe is sent
                    // here as well as inside `set_viewer_attached` because that call is a
                    // no-op when a viewer was already attached — a second device connecting
                    // still needs something its decoder can start from.
                    let _ = cmd_tx.send(CompositorCommand::ViewerAttached(true));
                    let _ = cmd_tx.send(CompositorCommand::ForceKeyframe);
                }
                // NOT a teardown. A peer connection dying is a *transport* event — a cell
                // handoff, a tunnel change, a few seconds in a lift — and killing the session
                // here meant every one of those cost the viewer their windows, their
                // applications and a cold Chrome launch. Measured 2026-09-12: rtt spiked to
                // 2191 ms at 17:57:32 and the session was gone at 17:57:33.
                //
                // `viewer_watchdog` is the only teardown now. It waits `VIEWER_GRACE` of
                // relay-link silence *and* a non-connected peer connection, so a viewer that
                // re-offers through the still-open relay socket keeps everything it had.
                RTCPeerConnectionState::Failed
                | RTCPeerConnectionState::Closed
                | RTCPeerConnectionState::Disconnected => {
                    if generation.load(Ordering::SeqCst) == my_gen {
                        // Stop rendering for a viewer that is not receiving. `Disconnected` is
                        // included on purpose: it is the transient case — a lift, a handoff —
                        // and it is precisely when there is no point encoding into a dead path.
                        // Resuming costs one keyframe.
                        let _ = cmd_tx.send(CompositorCommand::ViewerAttached(false));
                        info!(
                            "relay client: peer connection {state:?} — session kept, waiting for \
                             a re-offer (the watchdog stops it if no viewer comes back)"
                        );
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

/// The session the compositor is actually running, if any.
///
/// Asked of the compositor rather than read off `ctx.session_started`: that flag belongs to one
/// relay connection and a reconnecting viewer gets a fresh one set to `false`, so it cannot answer
/// "did a session survive my disconnect?" — which is the entire question here.
async fn live_session(ctx: &RelayCtx) -> Option<wado_protocol::SessionInfo> {
    let (tx, rx) = oneshot::channel();
    ctx.cmd_tx.send(CompositorCommand::Status { reply: tx }).ok()?;
    // Bounded like every other compositor round-trip here: a wedged render loop must not hold the
    // relay socket open waiting for an answer that is not coming.
    tokio::time::timeout(Duration::from_secs(2), rx).await.ok()?.ok().flatten()
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

/// How long a started session may go without any sign of its viewer before it is stopped.
///
/// Ten minutes, raised from 45 s once a detached session stopped costing anything to keep. The
/// old number was not chosen for the user's networks — it was chosen because a session with no
/// viewer still rendered and encoded 90 frames a second for nobody, so leaving one running was
/// expensive and reaping it quickly was the lesser evil. `ViewerAttached(false)` pauses the
/// render tick instead, which removes the reason to be stingy: what is left running is the
/// desktop and the applications the user put in it, which is the thing they asked to keep.
///
/// A dead zone on a commute routinely outlasted 45 s, so the old value reaped exactly the
/// sessions this branch exists to preserve.
const VIEWER_GRACE: std::time::Duration = std::time::Duration::from_secs(600);

/// Stop a session whose viewer has vanished without saying so.
///
/// **Why this exists.** Until now the *only* thing that stopped a session was the WebRTC peer
/// connection reaching `Failed` or `Closed`. That covers a viewer that closes its tab. It does
/// not cover the relay link dropping — a relay restart, a network blip, or (2026-09-12) a
/// panicked relay task — because the reconnect loop treats that as routine and retries. The
/// compositor, the encoder and every application the session launched keep running, with
/// nobody watching and nothing left that knows how to stop them. A browser playing audio into
/// an empty room is the same failure the process-group cleanup fixed, arriving by a different
/// door.
///
/// **Why two clocks and not one, and why relay silence is not one of them on its own.**
///
/// This first required only that the relay link had been silent for the grace period and that
/// WebRTC was not connected. That reads as two conditions and is really one, because **a healthy
/// viewer is silent on the relay link**: its media and its input are on WebRTC, and it speaks to
/// the relay only when something changes. Measured 2026-09-12 22:34:06 — ICE reached `Failed` and
/// the session was reaped **three seconds** later, with `silent_ms=47699`. The grace period had
/// already elapsed before the fault, so it granted no grace at all and the work that stopped the
/// peer-connection handler from tearing sessions down was undone one layer along.
///
/// So the first clock is now **how long it has been since a viewer was last actually connected**,
/// which is the thing "no viewer" was always trying to measure. Relay silence stays as the second
/// clock, and it earns its place: a viewer whose WebRTC is down but who is re-offering through the
/// relay right now is *present*, and killing the session it is trying to rejoin is the worst
/// available move. Both must be old before anything is stopped.
async fn viewer_watchdog(
    cmd_tx: CommandSender,
    last_connected: Arc<AtomicU64>,
    last_relay_msg: Arc<AtomicU64>,
    session_started: Arc<AtomicBool>,
    active_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tick.tick().await;
        if !session_started.load(Ordering::SeqCst) {
            continue;
        }
        // The lock is released before the await point; holding a std Mutex across one would
        // be a deadlock waiting for a slow tick.
        let connected = {
            let pc = active_pc.lock().unwrap_or_else(|e| e.into_inner()).clone();
            pc.map(|pc| pc.connection_state() == RTCPeerConnectionState::Connected)
                .unwrap_or(false)
        };
        // Sampled here as well as on the state change, so a long-lived connection keeps the clock
        // fresh without depending on an event that only fires on transitions.
        if connected {
            last_connected.store(now_ms(), Ordering::Relaxed);
        }
        let now = now_ms();
        let gone_ms = now.saturating_sub(last_connected.load(Ordering::Relaxed));
        let silent_ms = now.saturating_sub(last_relay_msg.load(Ordering::Relaxed));
        if !should_reap(gone_ms, silent_ms, connected) {
            continue;
        }
        warn!(
            gone_ms,
            silent_ms,
            "no viewer connected for {VIEWER_GRACE:?} and nothing on the relay link either — \
             stopping the session so its applications do not outlive it"
        );
        session_started.store(false, Ordering::SeqCst);
        let _ = cmd_tx.send(CompositorCommand::Stop);
    }
}

/// The watchdog's decision, split out so it can be tested without a session, a socket or a clock.
fn should_reap(gone_ms: u64, silent_ms: u64, connected: bool) -> bool {
    let grace = VIEWER_GRACE.as_millis() as u64;
    !connected && gone_ms >= grace && silent_ms >= grace
}

#[cfg(test)]
mod watchdog_tests {
    use super::{VIEWER_GRACE, should_reap};

    const GRACE: u64 = VIEWER_GRACE.as_millis() as u64;

    #[test]
    fn a_connected_viewer_is_never_reaped() {
        // However long it has been quiet: the media path is where a viewer lives.
        assert!(!should_reap(0, GRACE * 10, true));
    }

    #[test]
    fn relay_silence_alone_does_not_reap_a_recent_connection() {
        // The regression that shipped, measured 2026-09-12 22:34:06: ICE failed after 48 s of
        // ordinary relay quiet, and the session died three seconds later because the old rule
        // read that quiet as absence.
        assert!(!should_reap(3_000, 47_699, false));
    }

    #[test]
    fn a_viewer_re_offering_through_the_relay_keeps_its_session() {
        // WebRTC long gone, but the relay link is busy — that is someone trying to come back,
        // and it is the case the whole evening's reconnect work exists to serve.
        assert!(!should_reap(GRACE * 3, 1_000, false));
    }

    #[test]
    fn gone_by_every_route_is_reaped() {
        assert!(should_reap(GRACE, GRACE, false));
        assert!(should_reap(GRACE * 5, GRACE * 5, false));
    }

    #[test]
    fn just_under_the_grace_on_either_clock_is_kept() {
        assert!(!should_reap(GRACE - 1, GRACE * 2, false));
        assert!(!should_reap(GRACE * 2, GRACE - 1, false));
    }
}

