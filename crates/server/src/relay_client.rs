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
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{FutureExt, SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMsg;
use tracing::{debug, error, info, warn};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MediaEngine};
use webrtc::api::{API, APIBuilder};
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use wado_compositor::{CommandSender, CompositorCommand, FrameMsg, InputSender};
use wado_protocol::{relay::RelayMsg, relay::display_remote_id};

use crate::e2e::link::Inbound;
use crate::website::logbus::LogBus;

/// Reconnect backoff bounds.
const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
///
/// Capped at 5 s, down from 30: the relay is meant to be restarted rarely and reached fast, and
/// after a relay restart every daemon of every pool is waiting out this cap with a phone parked
/// on the other side. The jitter in the reconnect loop keeps a pool from redialling in lockstep.
const BACKOFF_MAX: Duration = Duration::from_secs(5);

/// The relay pings every 15 s; this long with nothing at all from it means the link is dead
/// even if TCP has not noticed — behind a tunnel it may never notice.
const RELAY_SILENCE: Duration = Duration::from_secs(45);

/// Everything a relay connection needs; lives across reconnects.
struct RelayCtx {
    api: Arc<API>,
    cmd_tx: CommandSender,
    log_bus: LogBus,
    /// The single active viewer's peer connection (generation-guarded).
    active_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
    /// The viewer's files peer connection — a second one, so transfers never share an SCTP
    /// association with input. See [`crate::files::channel`].
    files_pc: Arc<Mutex<Option<Arc<RTCPeerConnection>>>>,
    /// The viewer's trackpad, if it opened one (see [`crate::pad`]).
    pad: crate::pad::Live,
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
    /// The session's windows, forwarded as `RelayMsg::Windows`. Latest-value-wins state, the
    /// same shape as `text_input`.
    windows: tokio::sync::watch::Receiver<wado_protocol::WindowList>,
    /// The menu open on the focused window, read into a sheet for the phone (M-P S7).
    menu: tokio::sync::watch::Receiver<Option<wado_compositor::hit::MenuSpot>>,
    clipboard: tokio::sync::watch::Receiver<wado_compositor::clipboard::Clip>,
    /// Bitrate actually written to the video track over the last stretch, in kbps. Forwarded to
    /// the viewer — see [`wado_protocol::RelayMsg::SentKbps`] for why it has to be.
    sent_kbps: tokio::sync::watch::Receiver<u32>,
    /// Smoothed time (microseconds) encoded frames wait before the pump takes them — the
    /// `queue_ms` leg of `TimingRequest`. Only the pump knows it; the compositor reports zero.
    queue_us: Arc<AtomicU64>,
    /// Unix-millis of the last moment a viewer's peer connection was seen `Connected`. See
    /// [`viewer_watchdog`] — this, not relay silence, is how long there has been no viewer.
    last_connected: Arc<AtomicU64>,
    /// Unix-millis of the last message received from the relay. See [`viewer_watchdog`].
    last_relay_msg: Arc<AtomicU64>,
    /// Whether a session was started for a viewer and has not been stopped. See
    /// [`viewer_watchdog`].
    session_started: Arc<AtomicBool>,
    /// The viewer left on purpose ("Leave"): keep the session, do not reap it.
    detached: Arc<AtomicBool>,
    /// The running session's start time (unix ms) and shape, for the session directory.
    session_since: Arc<AtomicU64>,
    session_shape: Arc<Mutex<(u32, u32, u32)>>,
    /// Who the relay says is on the other end — `<addr> room=<first 8 of room_id>`.
    ///
    /// Only the relay knows this; the WebRTC half of the daemon otherwise logs offers, answers
    /// and ICE states with nothing identifying the device that caused them. With a pool of
    /// daemons and several phones that is unreadable: `scripts/watch.sh` resorts to using the
    /// offer's *candidate count* as a device fingerprint, and on 2026-09-14 the absence of this
    /// cost three wrong hypotheses about which daemon was poisoned. It is one string.
    peer: Arc<Mutex<String>>,
    /// The relay room of the viewer the relay last said is here, or empty. A `PeerDisconnected`
    /// for any other room is stale — the viewer it names was already replaced — and acting on
    /// it would close the *current* viewer's peer connection.
    room: Arc<Mutex<String>>,
    /// The viewer in `room` passed the gate: it may use the shells, files and trackpad its
    /// grants allow.
    viewer_ok: Arc<AtomicBool>,
    /// The trust list and the pairing codes — see [`crate::gate`].
    gate: crate::gate::Gate,
    /// Rooms a takeover check approved. Their `PeerConnected` is let in without asking twice.
    approved: Arc<Mutex<std::collections::HashSet<String>>>,
    /// The end-to-end envelope with the device on the seat — see [`crate::e2e`]. Every frame
    /// in or out of the relay link passes through it.
    link: Arc<Mutex<crate::e2e::link::Link>>,
    /// This computer's identity, shared by every daemon of the pool.
    host_key: Arc<crate::e2e::host_key::HostKey>,
    /// The session's participants — the seat's viewer is one of them, joined rooms the others
    /// (see [`crate::viewers`]). Owns the "listening" and "low-latency audio" switches too.
    hub: Arc<crate::viewers::Hub>,
    /// The seat's viewer's origin in the hub, or 0 when nobody is on the seat.
    seat_origin: Arc<AtomicU16>,
    /// The device key and name of the viewer in `room` — what its config edits and saved
    /// settings are filed under (see [`crate::config::link`]).
    viewer: Arc<Mutex<(String, String)>>,
    /// The cover art and icons this viewer already has — see [`crate::host::trim`].
    media_sent: Arc<crate::host::trim::Sent>,
    /// The accessibility-tree client that answers `TargetsRequest` — see [`crate::a11y`].
    /// Shared, because each question runs as its own task.
    a11y: Arc<crate::a11y::A11y>,
}

/// Now, in unix milliseconds.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Spawn the relay client on a dedicated thread. Mirrors `website::start`.
///
/// Takes the compositor's handles whole and unpacks them by name: a dozen receivers of
/// near-identical types passed by position is how the audio sink once received the D-Bus
/// address instead.
pub fn start(
    handles: wado_compositor::CompositorHandles,
    frame_rx: mpsc::Receiver<FrameMsg>,
    relay_url: String,
    remote_id: String,
    log_bus: LogBus,
) -> crate::Result<()> {
    std::thread::Builder::new()
        .name("wado-relay-client".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    error!("relay client: failed to build tokio runtime: {e}");
                    return;
                }
            };
            let wado_compositor::CompositorHandles {
                commands: cmd_tx,
                input: input_tx,
                timings,
                text_input,
                shedding,
                windows,
                menu,
                clipboard,
                app_bus,
            } = handles;
            rt.block_on(async move {
                // Daemon-lifetime, not connection-lifetime: notifications arrive whether or not
                // a viewer is connected at that moment.
                tokio::spawn(crate::notify::run(app_bus));
                // "Transfer finished" pushes, from this daemon's own lines of the files log.
                tokio::spawn(crate::push::transfer::follow());
                crate::clip::watch::start();
                crate::clip::sync::start(cmd_tx.clone(), clipboard.clone());
                if let Err(e) = run(
                    cmd_tx, input_tx, timings, text_input, shedding, windows, menu, clipboard,
                    frame_rx, relay_url, remote_id, log_bus,
                )
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
    windows: tokio::sync::watch::Receiver<wado_protocol::WindowList>,
    menu: tokio::sync::watch::Receiver<Option<wado_compositor::hit::MenuSpot>>,
    clipboard: tokio::sync::watch::Receiver<wado_compositor::clipboard::Clip>,
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
    // The "This phone" output and its track (see `crate::host`, `crate::audio`). Session apps
    // are pointed at the sink from the start, so a session plays on the phone by default.
    let audio_track = crate::audio::track();
    let (listening_tx, listening) = tokio::sync::watch::channel(false);
    let (audio_low_tx, audio_low) = tokio::sync::watch::channel(false);
    if let Some(sink) = crate::host::start() {
        let _ = cmd_tx.send(CompositorCommand::AudioSink(Some(sink.clone())));
        tokio::spawn(crate::audio::run(
            sink,
            listening,
            audio_low,
            Arc::clone(&audio_track),
        ));
    }

    // What actually leaves this process, measured at the track. Published for the viewer, which
    // otherwise sees only what arrived and must guess which end lost the difference.
    let (sent_kbps_tx, sent_kbps) = tokio::sync::watch::channel(0u32);
    let queue_us = Arc::new(AtomicU64::new(0));
    {
        let track_pump = Arc::clone(&track);
        let queue_us = Arc::clone(&queue_us);
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
                // picking it up — the leg the compositor explicitly cannot see. It is also
                // the leg that grows first when the pump falls behind, which makes it an
                // early warning rather than a post-mortem.
                let waited = frame.queued_at.elapsed();
                // EWMA, 1/8 weight on the newest sample — same as the direct-mode pump.
                let prev = queue_us.load(Ordering::Relaxed);
                queue_us.store(
                    (prev * 7 + waited.as_micros() as u64) / 8,
                    Ordering::Relaxed,
                );
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
                if frames.is_multiple_of(300) {
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
                // The duration matters: real elapsed time keeps the RTP clock on wall
                // clock; see ChannelSink.
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
                    if took > worst {
                        worst = took;
                    }
                    // Same once-per-60 cadence as the drop counter, so the two lines pair up.
                    if slow.is_multiple_of(60) {
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

    let a11y = Arc::new(crate::a11y::A11y::default());
    let gate = crate::gate::Gate::default();
    let hub = crate::viewers::Hub::new(
        Arc::clone(&api),
        Arc::clone(&track),
        Arc::clone(&audio_track),
        cmd_tx.clone(),
        input_tx,
        listening_tx,
        audio_low_tx,
        gate.clone(),
        remote_id.clone(),
        crate::viewers::Feeds {
            text_input: text_input.clone(),
            shedding: shedding.clone(),
            windows: windows.clone(),
            menu: menu.clone(),
            clipboard: clipboard.clone(),
            sent_kbps: sent_kbps.clone(),
            timings: timings.clone(),
            queue_us: Arc::clone(&queue_us),
            a11y: Arc::clone(&a11y),
        },
    );
    tokio::spawn(Arc::clone(&hub).run());
    tokio::spawn(crate::desks::sibling_reaper(Arc::clone(
        &hub.session_started,
    )));

    let ctx = RelayCtx {
        api,
        cmd_tx,
        log_bus,
        text_input,
        shedding,
        windows,
        menu,
        clipboard,
        sent_kbps,
        queue_us,
        active_pc: Arc::new(Mutex::new(None)),
        generation: Arc::new(AtomicU64::new(0)),
        last_connected: Arc::clone(&hub.last_connected),
        last_relay_msg: Arc::new(AtomicU64::new(now_ms())),
        session_started: Arc::clone(&hub.session_started),
        detached: Arc::clone(&hub.detached),
        session_since: Arc::default(),
        session_shape: Arc::default(),
        peer: Arc::new(Mutex::new("<none>".to_string())),
        room: Arc::new(Mutex::new(String::new())),
        viewer_ok: Arc::new(AtomicBool::new(false)),
        gate,
        approved: Arc::default(),
        link: Arc::default(),
        host_key: Arc::new(crate::e2e::host_key::HostKey::load_or_create(
            &crate::remote_id::config_dir(),
        )?),
        viewer: Arc::default(),
        files_pc: Arc::default(),
        pad: crate::pad::Live::default(),
        media_sent: Arc::default(),
        hub,
        seat_origin: Arc::default(),
        a11y,
        relay_url,
        remote_id,
        timings,
    };

    // Asked once, at startup, for the same reason the IPv6 probe is: it describes the host, not
    // the connection. A verdict here explains every later ICE failure at once.
    tokio::spawn(crate::nat::report());

    tokio::spawn(viewer_watchdog(
        ctx.cmd_tx.clone(),
        Arc::clone(&ctx.last_connected),
        Arc::clone(&ctx.last_relay_msg),
        Arc::clone(&ctx.session_started),
        Arc::clone(&ctx.active_pc),
        Arc::clone(&ctx.detached),
        Arc::clone(&ctx.hub),
    ));

    // The session directory (`crate::sessions`): this daemon's running session, rewritten every
    // few seconds as a heartbeat, removed when there is none.
    {
        let started = Arc::clone(&ctx.session_started);
        let detached = Arc::clone(&ctx.detached);
        let since = Arc::clone(&ctx.session_since);
        let shape = Arc::clone(&ctx.session_shape);
        let windows = ctx.windows.clone();
        let viewer = Arc::clone(&ctx.viewer);
        let active_pc = Arc::clone(&ctx.active_pc);
        let hub = Arc::clone(&ctx.hub);
        let instance = format!(
            "{}:{}",
            wado_protocol::relay::normalize_remote_id(&ctx.remote_id),
            crate::instance::instance_key()
        );
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(3));
            loop {
                tick.tick().await;
                if !started.load(Ordering::SeqCst) {
                    crate::sessions::clear(&instance);
                    continue;
                }
                let connected = active_pc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
                    .is_some_and(|pc| pc.connection_state() == RTCPeerConnectionState::Connected);
                let (width, height, fps) = hub
                    .shape()
                    .unwrap_or_else(|| *shape.lock().unwrap_or_else(|e| e.into_inner()));
                let apps = windows
                    .borrow()
                    .windows
                    .iter()
                    .map(|w| {
                        if w.app_id.is_empty() {
                            w.title.clone()
                        } else {
                            w.app_id.clone()
                        }
                    })
                    .collect();
                let (lead, participants, max_participants, kbps) = hub.summary();
                crate::sessions::write(&wado_protocol::SessionSummary {
                    instance: instance.clone(),
                    started_ms: since.load(Ordering::Relaxed),
                    age_s: 0,
                    width,
                    height,
                    fps,
                    apps,
                    viewer: connected
                        .then(|| viewer.lock().unwrap_or_else(|e| e.into_inner()).1.clone()),
                    detached: detached.load(Ordering::SeqCst),
                    lead,
                    participants,
                    max_participants,
                    kbps,
                });
            }
        });
    }

    // ── Reconnect loop ──────────────────────────────────────────────────────
    // The link's health, for `wado relay status`, the TUI and the relay-down push.
    crate::relay_watch::state::start(&ctx.relay_url);
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
        let attempt = AssertUnwindSafe(connect_and_serve(&ctx))
            .catch_unwind()
            .await;
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
                crate::relay_watch::state::failed(&e.to_string());
                if first_attempt {
                    warn!("relay client: cannot reach relay ({e}) — retrying in {backoff:?}");
                } else {
                    info!("relay client: relay still unreachable ({e}) — retrying in {backoff:?}");
                }
            }
        }
        first_attempt = false;
        let jitter = backoff.mul_f64(rand::random::<f64>() * 0.5);
        tokio::time::sleep(backoff + jitter).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// One relay connection: WS connect → Register → serve messages until the
/// connection drops. `Ok(())` means registration succeeded (connection ended
/// later); `Err` means we never registered.
/// Why the viewer on the seat may not use the shells, asked afresh each time: the viewer
/// changes, and so can `security { shell-access }`.
///
/// ponytail: reads the trust list per message and output chunk — a few hundred bytes; cache it
/// if a shell burst ever shows up in a profile.
///
/// Shells also need a fresh passkey unlock (`crate::passkey`, F3). `use_it`: this is the person
/// acting (opening, typing), which keeps the unlock alive; output streaming out is not.
fn shell_refusal(
    ctx: &RelayCtx,
    use_it: bool,
) -> impl Fn() -> Option<&'static str> + Send + 'static {
    let viewer = Arc::clone(&ctx.viewer);
    let ok = Arc::clone(&ctx.viewer_ok);
    let gate = ctx.gate.clone();
    move || {
        if !ok.load(Ordering::SeqCst) {
            return Some("this device has not been let in yet");
        }
        let key = viewer.lock().unwrap_or_else(|e| e.into_inner()).0.clone();
        crate::shells::access::refused(&gate, &key).or_else(|| {
            if use_it {
                crate::passkey::refused(&key)
            } else {
                crate::passkey::refused_quiet(&key)
            }
        })
    }
}

async fn connect_and_serve(ctx: &RelayCtx) -> crate::Result<()> {
    // ── 1. Connect ───────────────────────────────────────────────────────────
    let register_url = format!("{}/register", ctx.relay_url);
    let (ws_stream, _) = connect_async(&register_url)
        .await
        .map_err(|e| crate::WadoError::Other(format!("connect: {e}")))?;
    let (ws_sink, mut ws_stream) = ws_stream.split();

    // All outbound relay WS messages go through this channel so multiple tasks
    // can write without fighting over the SplitSink.
    //
    // This writer is also the envelope's one sealing point (`crate::e2e::link`): a frame for the
    // device leaves sealed, in counter order, or is held until the handshake finishes.
    //
    // And it is the lane's pacer (`crate::lane`): signalling first and at once, everything bulk
    // held under the relay's 64 KB/s cap — ordered *before* sealing, so the counters stay in order.
    ctx.link.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let (out_tx, out_rx) = mpsc::channel::<String>(128);
    let link = Arc::clone(&ctx.link);
    let outbox = crate::lane::Outbox::default();
    let lane = outbox.gauge();
    // Files with no direct path ride this link too (`crate::files::lane`).
    let files_lane = crate::files::lane::OnLane::default();
    let ws_sink = Arc::new(tokio::sync::Mutex::new(ws_sink));
    let gen_link = Arc::clone(&ctx.link);
    let generation = move || {
        gen_link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .generation()
    };
    let write_task = tokio::spawn(crate::lane::pump::run(
        out_rx,
        outbox,
        generation,
        move |text| {
            let frames = link
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .outbound(text);
            let ws_sink = Arc::clone(&ws_sink);
            async move {
                let mut sink = ws_sink.lock().await;
                for f in frames {
                    if sink.send(WsMsg::Text(f)).await.is_err() {
                        return false;
                    }
                }
                true
            }
        },
    ));

    // ── 2. Register ──────────────────────────────────────────────────────────
    // What this relay can do, from its `registered`. An older relay neither waits for this
    // daemon's verdict on a join nor pings it, so neither may be assumed.
    let relay_gates: bool;
    let relay_pings: bool;
    let relay_caps: Vec<String>;
    let display_name = std::env::var("HOSTNAME").ok();
    send_relay(
        &out_tx,
        &RelayMsg::Register {
            remote_id: ctx.remote_id.clone(),
            display_name,
            v: wado_protocol::relay::WIRE_VERSION,
            instance_key: crate::instance::instance_key(),
            boot_id: crate::instance::boot_id().to_string(),
            // The relay holds a dropped viewer's seat exactly as long as this daemon keeps the
            // desktop behind it, so the two can never disagree.
            hold_ms: VIEWER_GRACE.as_millis() as u64,
            // `ctl`: this daemon serves control-only links (`crate::control`) — a relay with the
            // `control` cap then sends a widget's link here instead of refusing it.
            caps: vec!["pong".into(), "gate".into(), "ctl".into()],
        },
    )
    .await?;

    match ws_stream.next().await {
        Some(Ok(WsMsg::Text(t))) => match serde_json::from_str::<RelayMsg>(&t) {
            Ok(RelayMsg::Registered {
                remote_id,
                relay_v,
                caps,
            }) => {
                relay_gates = caps.iter().any(|c| c == "gate");
                relay_pings = caps.iter().any(|c| c == "ping");
                relay_caps = caps.clone();
                // Tell the relay whether a session runs here, on every change, so a device
                // without a daemon of its own is handed an idle one. Ends with the link: its
                // sends fail once the write task's receiver is gone.
                {
                    let out_tx = out_tx.clone();
                    let started = Arc::clone(&ctx.session_started);
                    tokio::spawn(async move {
                        let mut last = None;
                        loop {
                            let now = started.load(Ordering::SeqCst);
                            if last != Some(now) {
                                if send_relay(&out_tx, &RelayMsg::Occupancy { session: now })
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                                last = Some(now);
                            }
                            if out_tx.is_closed() {
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    });
                }
                // relay_v 0 = a relay from before the handshake was versioned.
                info!(
                    "relay client: registered — Remote ID {} as instance {} (relay wire v{relay_v}, caps {caps:?})",
                    display_remote_id(&remote_id),
                    crate::instance::instance_key()
                );
            }
            Ok(RelayMsg::Error { message }) => {
                write_task.abort();
                return Err(crate::WadoError::Other(format!(
                    "relay rejected registration: {message}"
                )));
            }
            Ok(other) => {
                write_task.abort();
                return Err(crate::WadoError::Other(format!(
                    "unexpected relay response: {other:?}"
                )));
            }
            Err(e) => {
                write_task.abort();
                return Err(crate::WadoError::Other(format!(
                    "bad relay response JSON: {e}"
                )));
            }
        },
        _ => {
            write_task.abort();
            return Err(crate::WadoError::Other(
                "relay closed before Registered".into(),
            ));
        }
    }

    // Registered: "back online" is pushed if the computer was away (`crate::push::online`);
    // dropping this when the link ends marks when it went. Then the relay's own health
    // (`crate::relay_watch`) — after, because "back online" defers to its "relay is back".
    let _online = crate::push::online::registered();
    let _relay_up = crate::relay_watch::state::registered(&relay_caps);

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
    //
    // With several devices in the session, only the one whose input arrived last raises its
    // keyboard (`Hub::keyboard_for`) — otherwise every participant's phone would pop one.
    let text_input_task = {
        let mut rx = ctx.text_input.clone();
        let mut last = ctx.hub.last_input.subscribe();
        let (hub, seat) = (Arc::clone(&ctx.hub), Arc::clone(&ctx.seat_origin));
        let out_tx_ti = out_tx.clone();
        tokio::spawn(async move {
            // Mark the held value unseen so the first iteration sends it rather than waiting.
            rx.mark_changed();
            let mut sent = None;
            loop {
                tokio::select! {
                    r = rx.changed() => if r.is_err() { break },
                    r = last.changed() => if r.is_err() { break },
                }
                last.borrow_and_update();
                let active =
                    *rx.borrow_and_update() && hub.keyboard_for(seat.load(Ordering::SeqCst));
                if sent != Some(active) {
                    sent = Some(active);
                    let _ = send_relay_nowait(&out_tx_ti, &RelayMsg::TextInput { active }).await;
                }
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

    // Same shape as the three above. Awaited rather than `nowait`: a dropped log line is
    // replaced by the next one, but a dropped window list stays wrong until something else
    // changes — the bar would show a closed app until the next focus change.
    let windows_task = {
        let mut rx = ctx.windows.clone();
        let out_tx_w = out_tx.clone();
        tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let list = rx.borrow_and_update().clone();
                if send_relay(
                    &out_tx_w,
                    &RelayMsg::Windows {
                        windows: list.windows,
                        workspace: list.workspace,
                    },
                )
                .await
                .is_err()
                {
                    break;
                }
            }
        })
    };

    // The config as this viewer sees it, again after every reload — so a limit changed in
    // config.kdl, or a broken save, reaches the control centre without a refresh.
    let config_task = {
        let mut rx = crate::config::status();
        let out_tx_c = out_tx.clone();
        let viewer = Arc::clone(&ctx.viewer);
        let gate = ctx.gate.clone();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                rx.borrow_and_update();
                let key = viewer.lock().unwrap_or_else(|e| e.into_inner()).0.clone();
                let state = crate::config::link::state_for(&key, &gate);
                if send_relay(&out_tx_c, &RelayMsg::ConfigState { state })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
    };

    // What other devices of this computer did to its files — a toast for this viewer.
    let files_note_task = {
        let out_tx_f = out_tx.clone();
        let ok = Arc::clone(&ctx.viewer_ok);
        tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel(16);
            // Joined, not spawned: aborting this task stops the follower with it.
            let forward = async move {
                while let Some((device, op, path)) = rx.recv().await {
                    if ok.load(Ordering::SeqCst)
                        && send_relay(&out_tx_f, &RelayMsg::FilesNote { device, op, path })
                            .await
                            .is_err()
                    {
                        break;
                    }
                }
            };
            tokio::join!(crate::files::audit::follow(tx), forward);
        })
    };

    // Notifications from the session's apps, as they happen.
    let notify_task = {
        let mut rx = crate::notify::events();
        let out_tx_n = out_tx.clone();
        tokio::spawn(async move {
            use crate::notify::Event;
            loop {
                let msg = match rx.recv().await {
                    Ok(Event::Shown {
                        id,
                        app,
                        summary,
                        body,
                    }) => RelayMsg::Notification {
                        id,
                        app,
                        summary,
                        body,
                    },
                    Ok(Event::Closed { id }) => RelayMsg::NotificationClosed { id },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                if send_relay(&out_tx_n, &msg).await.is_err() {
                    break;
                }
            }
        })
    };

    // The viewer asked for every new copy in full (`ClipWatch { live }`) — its "Copy from this
    // computer automatically". Reset for each new viewer, who asks for itself.
    let clip_live = Arc::new(AtomicBool::new(false));

    // The session clipboard, whenever an app copies, as text — for a viewer that is not getting
    // every copy in full already (`clip_task`): the session's own copy and paste, which needs no
    // grant. Never a secret. Only changes are sent — `mark_changed` is not called, so a viewer
    // that connects is not handed whatever was copied an hour ago.
    let clipboard_task = {
        let mut rx = ctx.clipboard.clone();
        let out_tx_cb = out_tx.clone();
        let live = Arc::clone(&clip_live);
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let clip = rx.borrow_and_update().clone();
                // Images reach the viewer through the host history (`clip`), not this.
                if !clip.is_text() || clip.secret || live.load(Ordering::SeqCst) {
                    continue;
                }
                let text = String::from_utf8_lossy(&clip.data).into_owned();
                if send_relay(&out_tx_cb, &RelayMsg::Clipboard { text })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
    };

    // The computer's clipboard for a viewer with the grant: its history again on every change,
    // and every new copy in full once it asks (`clip_live`).
    let clip_task = {
        let (viewer, ok, gate, live) = (
            Arc::clone(&ctx.viewer),
            Arc::clone(&ctx.viewer_ok),
            ctx.gate.clone(),
            Arc::clone(&clip_live),
        );
        crate::clip::share::follow(out_tx.clone(), move || {
            let key = viewer.lock().unwrap_or_else(|e| e.into_inner()).0.clone();
            crate::clip::share::Listen {
                granted: ok.load(Ordering::SeqCst) && gate.grants(&key).clipboard,
                key,
                watching: true,
                live: live.load(Ordering::SeqCst),
            }
        })
    };

    // Every shell's output and exits, for whoever is connected now. The shells themselves
    // outlive this connection (see `crate::shells`).
    let shells_task = {
        let mut rx = crate::shells::events();
        let out_tx_s = out_tx.clone();
        let refusal = shell_refusal(ctx, false);
        let lane = lane.clone();
        tokio::spawn(async move {
            use crate::shells::Event;
            loop {
                // A big dump waits for the lane (`crate::lane::gauge`) rather than queueing
                // minutes of output behind a 64 KB/s link; what overflows meanwhile is the lag
                // below, said in-band.
                lane.room().await;
                let ev = rx.recv().await;
                if ev.is_ok() && refusal().is_some() {
                    continue;
                }
                let msg = match ev {
                    Ok(Event::Output { id, data }) => RelayMsg::PtyOutput {
                        id,
                        data,
                        replay: false,
                    },
                    Ok(Event::Exit { id }) => RelayMsg::PtyExit { id, code: None },
                    Ok(Event::Changed) => RelayMsg::Shells {
                        shells: crate::shells::list(),
                        hosts: crate::shells::hosts::list(),
                    },
                    // Fell behind a burst of output: say so in-band rather than silently skip.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(n, "shell output lagged for this viewer");
                        continue;
                    }
                    Err(_) => break,
                };
                if send_relay(&out_tx_s, &msg).await.is_err() {
                    break;
                }
            }
        })
    };

    // Claude's cards, on every change, for a device with the `shells` grant (`crate::claude`).
    let claude_task = {
        let (viewer, ok, gate) = (
            Arc::clone(&ctx.viewer),
            Arc::clone(&ctx.viewer_ok),
            ctx.gate.clone(),
        );
        crate::claude::relay::follow(out_tx.clone(), move || {
            let key = viewer.lock().unwrap_or_else(|e| e.into_inner()).0.clone();
            ok.load(Ordering::SeqCst) && crate::shells::access::refused(&gate, &key).is_none()
        })
    };

    // The open menu, as a sheet. Same shape as `windows_task`, plus the tree read in between:
    // each popup is read once, and a popup that closes before its read finishes is simply
    // superseded by the `None` that follows.
    let menu_task = {
        let mut rx = ctx.menu.clone();
        let out_tx_m = out_tx.clone();
        let a11y = Arc::clone(&ctx.a11y);
        tokio::spawn(async move {
            rx.mark_changed();
            while rx.changed().await.is_ok() {
                let spot = rx.borrow_and_update().clone();
                let menu = match &spot {
                    Some(spot) => Some(crate::menu_sheet::read(&a11y, spot).await),
                    None => None,
                };
                if send_relay(&out_tx_m, &RelayMsg::Menu { menu })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        })
    };

    info!("relay client: ready — clients can connect with the Remote ID");

    // The viewer's interactive shell, if they have opened one. A local rather than
    // connection state held elsewhere, so that losing the connection drops it — and
    // dropping a `Pty` kills the shell and, through SIGHUP, whatever it was running.

    // Control-only links (`crate::control`): a phone's widgets, served beside the seat, never
    // on it. They live and die with this relay connection.
    let ctl_rooms = crate::control::Rooms::default();
    let ctl = crate::control::Shared {
        gate: ctx.gate.clone(),
        host_key: Arc::clone(&ctx.host_key),
        cmd_tx: Some(ctx.cmd_tx.clone()),
        relay: out_tx.clone(),
        hub: Some(Arc::clone(&ctx.hub)),
    };

    // ── 4. Main message loop ─────────────────────────────────────────────────
    loop {
        let frame = if relay_pings {
            match tokio::time::timeout(RELAY_SILENCE, ws_stream.next()).await {
                Ok(f) => f,
                Err(_) => {
                    warn!(
                        "relay client: nothing from the relay for {RELAY_SILENCE:?} — reconnecting"
                    );
                    break;
                }
            }
        } else {
            ws_stream.next().await
        };
        let Some(frame) = frame else { break };
        let text = match frame {
            Ok(WsMsg::Text(t)) => t,
            Ok(WsMsg::Ping(_)) | Ok(WsMsg::Pong(_)) => continue,
            Ok(WsMsg::Close(_)) => break,
            Ok(_) => continue,
            Err(e) => {
                warn!("relay client: WS error: {e}");
                break;
            }
        };

        // A control-only link's frame: its own room, its own envelope, never the seat's. Taken
        // before the liveness bump below, so a widget polling never keeps an abandoned session
        // alive.
        if text.starts_with(r#"{"type":"ctl"#) {
            match serde_json::from_str::<RelayMsg>(&text) {
                Ok(RelayMsg::CtlOpen {
                    room_id,
                    client_key,
                    client_name,
                    ..
                }) => ctl_rooms.open(&ctl, room_id, client_key, client_name).await,
                Ok(RelayMsg::Ctl { room_id, text }) => ctl_rooms.frame(&ctl, &room_id, &text).await,
                Ok(RelayMsg::CtlClose { room_id, .. }) => ctl_rooms.close(&room_id),
                _ => warn!("relay client: an unreadable control-link frame — dropped"),
            }
            continue;
        }

        // Liveness for `viewer_watchdog`. Bumped on every frame, including ones we do not
        // understand — the point is that the relay link is carrying traffic, not what it says.
        //
        // The relay's own pings are excluded, and must stay excluded: since `2026-10-02` the
        // relay pings this daemon every 15 s, and counting those would keep `silent_ms` near
        // zero forever — the watchdog would never reap anything. Verified live 2026-09-19,
        // before pings: an abandoned session was reaped at exactly `VIEWER_GRACE`.
        if text.trim() != r#"{"type":"ping"}"# {
            ctx.last_relay_msg.store(now_ms(), Ordering::Relaxed);
        }

        // The envelope: the relay's own frames pass, the device's arrive sealed and are opened,
        // and nothing else from the device side is acted on. See `crate::e2e`.
        let sorted = ctx
            .link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .inbound(&text);
        let text = match sorted {
            Inbound::Relay => text,
            Inbound::Msg(m) => m,
            Inbound::Hello { v, eph } => {
                let reply = ctx.link.lock().unwrap_or_else(|e| e.into_inner()).hello(
                    &ctx.host_key,
                    v,
                    &eph,
                );
                if let Some(m) = reply {
                    out_tx.send(serde_json::to_string(&m)?).await.ok();
                }
                continue;
            }
            Inbound::Finish {
                dev_pk,
                sig,
                pair_mac,
                typed_mac,
            } => {
                e2e_finish(ctx, &out_tx, &dev_pk, &sig, &pair_mac, &typed_mac).await;
                continue;
            }
            Inbound::Drop(why) => {
                let kind = crate::e2e::link::kind(&text).unwrap_or("?");
                warn!("e2e: dropped a frame from the relay link ({kind}) — {why}");
                continue;
            }
        };

        let msg = match serde_json::from_str::<RelayMsg>(&text) {
            Ok(m) => m,
            Err(e) => {
                // Answered, not just logged. A dropped message leaves the sender waiting out a
                // timeout with no way to tell "the daemon rejected this" from "the daemon is
                // wedged" — which cost a debugging cycle here on 2026-09-13, when a probe sent
                // the wrong shape for `Quality` and simply hung.
                warn!("relay client: bad JSON from relay: {e}");
                send_relay(
                    &out_tx,
                    &RelayMsg::SessionError {
                        message: format!("this daemon could not understand that message: {e}"),
                    },
                )
                .await
                .ok();
                continue;
            }
        };

        match msg {
            RelayMsg::PeerConnected {
                room_id,
                client_addr,
                client_key,
                client_name,
                pair,
            } => {
                // Remembered, not just logged: every WebRTC line below is tagged with it.
                let short: String = room_id.chars().take(8).collect();
                *ctx.peer.lock().unwrap_or_else(|e| e.into_inner()) =
                    format!("{client_addr} room={short}");
                *ctx.room.lock().unwrap_or_else(|e| e.into_inner()) = room_id.clone();
                ctx.link
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .reset(&room_id, &client_key);
                *ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()) =
                    (client_key.clone(), client_name.clone());
                // The seat's viewer is a participant like a joined room's: a fresh origin, so the
                // last one's held input, place and dot go with it.
                {
                    let origin = ctx.hub.origin();
                    let old = ctx.seat_origin.swap(origin, Ordering::SeqCst);
                    if old != 0 {
                        ctx.hub.detach(old);
                    }
                    ctx.hub.attach(
                        origin,
                        &client_key,
                        &client_name,
                        out_tx.clone(),
                        Some(room_id.clone()),
                    );
                }
                // A new viewer never inherits the last one's trackpad or what it held down.
                ctx.pad.close();
                // Nor its passkey unlock: a new link proves itself again — unless the device chose a
                // longer one, which is bound to its key, not to a link (`crate::passkey::stay`).
                crate::passkey::unlock::lock_link();
                // The audio sink appears for this device, named after it.
                tokio::spawn(crate::host::viewer(Some(client_name.clone())));
                ctx.media_sent.clear();
                clip_live.store(false, Ordering::SeqCst);
                ctx.viewer_ok.store(!relay_gates, Ordering::SeqCst);
                info!(room_id = %room_id, client = %client_addr, device = %client_name, "relay client: peer connected");
                if relay_gates {
                    let pre_approved = ctx
                        .approved
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&room_id);
                    if pre_approved {
                        ctx.viewer_ok.store(true, Ordering::SeqCst);
                        send_relay(&out_tx, &RelayMsg::PeerAccept { room_id })
                            .await
                            .ok();
                    } else {
                        let room_now = Arc::clone(&ctx.room);
                        let ok = Arc::clone(&ctx.viewer_ok);
                        let rid_now = room_id.clone();
                        tokio::spawn(decide_join(
                            ctx.gate.clone(),
                            out_tx.clone(),
                            Join {
                                room_id,
                                key: client_key,
                                name: client_name,
                                addr: client_addr,
                                pair,
                            },
                            false,
                            move |_| {
                                // Only if it is still the viewer here — it may have left.
                                if *room_now.lock().unwrap_or_else(|e| e.into_inner()) == rid_now {
                                    ok.store(true, Ordering::SeqCst);
                                }
                            },
                        ));
                    }
                }
            }

            RelayMsg::PeerCheck {
                room_id,
                client_addr,
                client_key,
                client_name,
                pair,
            } => {
                // A device asking to take the seat from the viewer here. Decided exactly like a
                // join, but without touching `room` — the current viewer is still the current
                // viewer until the relay says otherwise.
                info!(room_id = %room_id, device = %client_name, "relay client: takeover check");
                let approved = Arc::clone(&ctx.approved);
                tokio::spawn(decide_join(
                    ctx.gate.clone(),
                    out_tx.clone(),
                    Join {
                        room_id,
                        key: client_key,
                        name: client_name,
                        addr: client_addr,
                        pair,
                    },
                    true,
                    move |room_id| {
                        approved
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(room_id.to_string());
                    },
                ));
            }

            RelayMsg::ApproveAnswer { .. } => {
                // From a client older than pairing-only (`2026-10-27`): no device approves
                // another any more.
                warn!("relay client: an approval answer — remote approval is gone; ignored");
            }

            RelayMsg::PeerDisconnected { room_id } => {
                // A check-approved room that never connected.
                ctx.approved
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(&room_id);
                {
                    let mut room = ctx.room.lock().unwrap_or_else(|e| e.into_inner());
                    if *room != room_id {
                        info!(room_id = %room_id, "relay client: departure of a viewer already replaced — ignored");
                        continue;
                    }
                    room.clear();
                }
                ctx.link.lock().unwrap_or_else(|e| e.into_inner()).clear();
                crate::links::clear(&ctx.remote_id);
                ctx.viewer_ok.store(false, Ordering::SeqCst);
                // Nobody back within 30 s and no session to keep playing into it: the sink goes,
                // so the computer's mixer shows no output for a device that is not there.
                {
                    let room = Arc::clone(&ctx.room);
                    let started = Arc::clone(&ctx.session_started);
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        let empty = room.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
                        if empty && !started.load(Ordering::SeqCst) {
                            crate::host::viewer(None).await;
                        }
                    });
                }
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
                //
                // Out of the session, too: what it held is released and the others are told. The
                // session's attached state is everyone's (`crate::viewers`), so a joined device
                // still watching keeps it rendering.
                let origin = ctx.seat_origin.swap(0, Ordering::SeqCst);
                if origin != 0 {
                    ctx.hub.detach(origin);
                } else {
                    let _ = ctx.cmd_tx.send(CompositorCommand::ViewerAttached(false));
                }
                let dead = ctx
                    .active_pc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take();
                // Files go with the device: no transfer outlives the link that authorised it.
                close_files(&ctx.files_pc);
                ctx.pad.close(); // and the trackpad: no key stays held after the device leaves
                crate::passkey::unlock::lock_link();
                if let Some(pc) = dead {
                    tokio::spawn(async move {
                        if let Err(e) = pc.close().await {
                            warn!("relay client: closing a departed viewer's peer connection: {e}");
                        }
                    });
                }
                // Nothing to keep the seat for: no session, or one left running (resumable by any
                // device from the list). Holding it anyway is how one phone's closed tabs filled
                // the whole pool and every other device was refused (2026-10-03).
                if !ctx.session_started.load(Ordering::SeqCst)
                    || ctx.detached.load(Ordering::SeqCst)
                {
                    send_relay(
                        &out_tx,
                        &RelayMsg::SeatHold {
                            room_id: room_id.clone(),
                            hold_ms: 0,
                        },
                    )
                    .await
                    .ok();
                    info!(room_id = %room_id, "relay client: viewer disconnected — no session to hold, seat freed");
                    continue;
                }
                info!(
                    room_id = %room_id,
                    "relay client: viewer disconnected — session kept, {VIEWER_GRACE:?} of grace \
                     starts from the last moment it was connected"
                );
            }

            RelayMsg::SessionStart { config } => {
                ctx.hub.audio_low.send_if_modified(|v| {
                    std::mem::replace(v, config.low_latency_audio) != config.low_latency_audio
                });
                if let Err(why) = config.validate() {
                    warn!("relay client: refusing an invalid session config: {why}");
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: format!("that configuration cannot work: {why}"),
                        },
                    )
                    .await
                    .ok();
                    continue;
                }
                // A session already running is not an error, it is a choice. Telling the viewer
                // "a session is already active" and closing the socket — which is what this did —
                // left a reconnecting phone with a session it could see in the logs and no way to
                // reach. See `RelayMsg::SessionAlive`.
                if let Some(info) = live_session(ctx).await {
                    info!("relay client: a session is already running — offering rejoin or drop");
                    send_relay(&out_tx, &RelayMsg::SessionAlive { info })
                        .await
                        .ok();
                    continue;
                }
                // The speed test's say (the user's decision): a new session that would not work
                // — no memory, the computer flat out, every session it runs already running — is
                // refused, plainly. Its last test if it ran one, else the computer alone.
                {
                    let key = ctx
                        .viewer
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                        .clone();
                    let v = ctx
                        .hub
                        .verdict(&key, crate::linktest::verdict::Intent::Start)
                        .unwrap_or_else(|| {
                            crate::viewers::room::verdict(&ctx.hub, &key, 0, 0, "start")
                        });
                    if v.refused() {
                        let why = v.reasons.first().cloned().unwrap_or_default();
                        warn!("relay client: refusing a new session — {why}");
                        send_relay(
                            &out_tx,
                            &RelayMsg::SessionError {
                                message: format!("a new session would not work here: {why}"),
                            },
                        )
                        .await
                        .ok();
                        continue;
                    }
                }
                ctx.session_started.store(true, Ordering::SeqCst);
                ctx.detached.store(false, Ordering::SeqCst);
                ctx.session_since.store(now_ms(), Ordering::Relaxed);
                *ctx.session_shape.lock().unwrap_or_else(|e| e.into_inner()) =
                    (config.width, config.height, config.fps);
                // The grace period starts now: a viewer that asked for a session but never
                // completes ICE must still be reaped, and it has never been connected.
                ctx.last_connected.store(now_ms(), Ordering::Relaxed);
                let (reply_tx, reply_rx) = oneshot::channel();
                let started_cfg = config.clone();
                if ctx
                    .cmd_tx
                    .send(CompositorCommand::Start {
                        config,
                        reply: reply_tx,
                    })
                    .is_err()
                {
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: "compositor unavailable".into(),
                        },
                    )
                    .await
                    .ok();
                    continue;
                }
                match tokio::time::timeout(Duration::from_secs(5), reply_rx).await {
                    Ok(Ok(Ok(info))) => {
                        // The seat's viewer started it: it leads, alone.
                        ctx.hub
                            .start(ctx.seat_origin.load(Ordering::SeqCst), &started_cfg);
                        ctx.hub.note_session(&info, true);
                        send_relay(&out_tx, &RelayMsg::SessionStarted { info })
                            .await
                            .ok();
                    }
                    Ok(Ok(Err(msg))) => {
                        send_relay(&out_tx, &RelayMsg::SessionError { message: msg })
                            .await
                            .ok();
                    }
                    Ok(Err(_)) => {
                        send_relay(
                            &out_tx,
                            &RelayMsg::SessionError {
                                message: "compositor dropped reply".into(),
                            },
                        )
                        .await
                        .ok();
                    }
                    Err(_) => {
                        send_relay(
                            &out_tx,
                            &RelayMsg::SessionError {
                                message: "session start timed out".into(),
                            },
                        )
                        .await
                        .ok();
                    }
                }
            }

            RelayMsg::SessionRejoin => {
                match live_session(ctx).await {
                    Some(info) => {
                        // A rejoin is a join: it displaces nobody. The seat's own resume, so no
                        // grant asked — but a full session is full.
                        if let Err(message) =
                            ctx.hub.join(ctx.seat_origin.load(Ordering::SeqCst), false)
                        {
                            send_relay(&out_tx, &RelayMsg::SessionError { message })
                                .await
                                .ok();
                            continue;
                        }
                        ctx.session_started.store(true, Ordering::SeqCst);
                        ctx.detached.store(false, Ordering::SeqCst);
                        ctx.last_connected.store(now_ms(), Ordering::Relaxed);
                        // The per-viewer reset (strain, shed divisor, congestion window) and the
                        // keyframe used to be sent from here by hand. They belong to
                        // `ViewerAttached`, which the peer-connection handler sends when this
                        // viewer's media path actually comes up — the rejoin message itself only
                        // means the button was pressed.
                        info!("relay client: viewer rejoined the running session");
                        send_relay(&out_tx, &RelayMsg::SessionStarted { info })
                            .await
                            .ok();
                    }
                    None => {
                        // The window between being offered the choice and taking it is real: the
                        // viewer watchdog stops idle sessions, so the thing being joined can be
                        // gone by the time the button is pressed.
                        send_relay(
                            &out_tx,
                            &RelayMsg::SessionError {
                                message: "the session ended before you could rejoin it".into(),
                            },
                        )
                        .await
                        .ok();
                    }
                }
            }

            RelayMsg::SessionReconfigure { config } => {
                // Same guard as `SessionStart`, from the same `SessionConfig::validate` — the
                // two verbs take the same struct from the same untrusted socket, and a check on
                // one of them is a check the other silently does not have.
                if let Err(why) = config.validate() {
                    warn!("relay client: refusing an invalid reconfigure: {why}");
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: format!("that configuration cannot work: {why}"),
                        },
                    )
                    .await
                    .ok();
                    continue;
                }
                // The lead's reshapes the session; anyone else's is only the bitrate it can
                // take — the output stays the lead's size (`crate::viewers`).
                let origin = ctx.seat_origin.load(Ordering::SeqCst);
                let lead = ctx.hub.is_lead(origin);
                let Some(config) = ctx.hub.reconfigure(origin, &config) else {
                    let msg = match live_session(&ctx).await {
                        Some(info) => RelayMsg::SessionReconfigured { info },
                        None => RelayMsg::SessionError {
                            message: "there is no session to change".into(),
                        },
                    };
                    send_relay(&out_tx, &msg).await.ok();
                    continue;
                };
                if lead {
                    ctx.hub.audio_low.send_if_modified(|v| {
                        std::mem::replace(v, config.low_latency_audio) != config.low_latency_audio
                    });
                    *ctx.session_shape.lock().unwrap_or_else(|e| e.into_inner()) =
                        (config.width, config.height, config.fps);
                }
                // No `session_started` reply and no renegotiation — see
                // `RelayMsg::SessionReconfigure`. The viewer keeps the peer connection it has;
                // the picture changes shape at the forced IDR.
                let (reply_tx, reply_rx) = oneshot::channel();
                if ctx
                    .cmd_tx
                    .send(CompositorCommand::Reconfigure {
                        config,
                        reply: reply_tx,
                    })
                    .is_err()
                {
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: "compositor unavailable".into(),
                        },
                    )
                    .await
                    .ok();
                    continue;
                }
                // The same bound every other compositor round-trip here uses: rebuilding an
                // encoder can fail slowly, and a wedged render loop must not hold the relay
                // socket waiting for an answer that is not coming.
                match tokio::time::timeout(Duration::from_secs(5), reply_rx).await {
                    Ok(Ok(Ok(info))) => {
                        ctx.hub.note_session(&info, lead);
                        send_relay(&out_tx, &RelayMsg::SessionReconfigured { info })
                            .await
                            .ok();
                    }
                    Ok(Ok(Err(msg))) => {
                        send_relay(&out_tx, &RelayMsg::SessionError { message: msg })
                            .await
                            .ok();
                    }
                    Ok(Err(_)) => {
                        send_relay(
                            &out_tx,
                            &RelayMsg::SessionError {
                                message: "compositor dropped reply".into(),
                            },
                        )
                        .await
                        .ok();
                    }
                    Err(_) => {
                        send_relay(
                            &out_tx,
                            &RelayMsg::SessionError {
                                message: "reconfigure timed out".into(),
                            },
                        )
                        .await
                        .ok();
                    }
                }
            }

            RelayMsg::SessionStop => {
                // Logged on receipt, not just on the watchdog's own reap: this is the only other
                // path to `CompositorCommand::Stop` in relay mode, and until now neither side
                // logged anything for it — a stop here and a stop from the grace expiring
                // were indistinguishable in the log, which cost real time chasing an unexplained
                // teardown on 2026-09-13 that turned out to be unprovable either way.
                info!("relay client: session_stop received from the viewer — tearing down");
                // Anyone may end it for everyone (the user's decision); the others are told who.
                ctx.hub.end(Some(ctx.seat_origin.load(Ordering::SeqCst)));
                ctx.session_started.store(false, Ordering::SeqCst);
                let _ = ctx.cmd_tx.send(CompositorCommand::Stop);
                send_relay(&out_tx, &RelayMsg::SessionStopped).await.ok();
            }

            RelayMsg::ViewerVisible { visible } => {
                ctx.hub
                    .visible(ctx.seat_origin.load(Ordering::SeqCst), visible);
            }

            RelayMsg::ViewerStrain { strained } => {
                // To the compositor through the hub: with several viewers, any one strained
                // sheds for the stream. No rate limiting: the client sends this only when its
                // settled verdict changes, and the render loop reads it once per decision window.
                ctx.hub
                    .strain(ctx.seat_origin.load(Ordering::SeqCst), strained);
            }

            // ── Several devices in one session (`crate::viewers`) ──
            RelayMsg::RoomHello { .. } => {}
            RelayMsg::LinkTest { bytes } => crate::linktest::send(out_tx.clone(), bytes),
            RelayMsg::LinkReport {
                down_kbps,
                rtt_ms,
                intent,
            } => {
                let key = ctx
                    .viewer
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .0
                    .clone();
                let verdict =
                    crate::viewers::room::verdict(&ctx.hub, &key, down_kbps, rtt_ms, &intent);
                send_relay(&out_tx, &RelayMsg::LinkVerdict { verdict })
                    .await
                    .ok();
            }
            RelayMsg::DeskRequest => {
                let (out_tx, remote_id, busy) = (
                    out_tx.clone(),
                    ctx.remote_id.clone(),
                    ctx.session_started.load(Ordering::SeqCst),
                );
                tokio::spawn(async move {
                    let rid = wado_protocol::relay::normalize_remote_id(&remote_id);
                    let msg = match crate::desks::request(&remote_id, busy).await {
                        crate::desks::Desk::Go(k) => RelayMsg::SessionElsewhere {
                            instance: format!("{rid}:{k}"),
                        },
                        crate::desks::Desk::Full { running, max } => RelayMsg::DesksFull {
                            running,
                            max,
                            message: format!(
                                "all {max} sessions this computer runs are in use — end one, or \
                                 join one"
                            ),
                        },
                        crate::desks::Desk::Failed(message) => RelayMsg::SessionError { message },
                    };
                    send_relay(&out_tx, &msg).await.ok();
                });
            }
            RelayMsg::SessionJoin => {
                let msg = match live_session(&ctx).await {
                    None => RelayMsg::SessionError {
                        message: "that session has ended".into(),
                    },
                    Some(info) => {
                        match ctx.hub.join(ctx.seat_origin.load(Ordering::SeqCst), true) {
                            Ok(()) => {
                                ctx.detached.store(false, Ordering::SeqCst);
                                RelayMsg::SessionStarted { info }
                            }
                            Err(message) => RelayMsg::SessionError { message },
                        }
                    }
                };
                send_relay(&out_tx, &msg).await.ok();
            }
            RelayMsg::SessionTakeover { config } => {
                let origin = ctx.seat_origin.load(Ordering::SeqCst);
                let taken = match config.validate() {
                    Err(why) => Err(format!("that configuration cannot work: {why}")),
                    Ok(()) if live_session(&ctx).await.is_none() => {
                        Err("that session has ended".to_string())
                    }
                    Ok(()) => ctx.hub.takeover(origin, &config),
                };
                let msg = match taken {
                    Err(message) => RelayMsg::SessionError { message },
                    Ok(cfg) => {
                        ctx.detached.store(false, Ordering::SeqCst);
                        *ctx.session_shape.lock().unwrap_or_else(|e| e.into_inner()) =
                            (cfg.width, cfg.height, cfg.fps);
                        match crate::viewers::room::reconfigure(&ctx.cmd_tx, cfg).await {
                            Ok(info) => {
                                ctx.hub.note_session(&info, true);
                                RelayMsg::SessionStarted { info }
                            }
                            Err(message) => RelayMsg::SessionError { message },
                        }
                    }
                };
                send_relay(&out_tx, &msg).await.ok();
            }

            RelayMsg::PadOffer { sdp } => {
                // Spawned: gathering takes up to `GATHER_WAIT`, and relayed touches arriving
                // meanwhile must not wait behind it.
                let (pad, api, peer, out_tx) = (
                    ctx.pad.clone(),
                    Arc::clone(&ctx.api),
                    pad_peer(ctx),
                    out_tx.clone(),
                );
                tokio::spawn(async move {
                    let reply = match pad.open(&api, &sdp, peer, out_tx.clone()).await {
                        Ok(sdp) => RelayMsg::PadAnswer {
                            sdp,
                            err: String::new(),
                        },
                        Err(err) => {
                            info!("pad: direct link refused — {err}");
                            RelayMsg::PadAnswer {
                                sdp: String::new(),
                                err,
                            }
                        }
                    };
                    send_relay(&out_tx, &reply).await.ok();
                });
            }

            RelayMsg::PadIn { msg } => {
                ctx.pad.relayed(msg, &pad_peer(ctx), &out_tx);
            }

            RelayMsg::FilesOffer { sdp } => {
                let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let who = crate::files::channel::Who { key, name };
                let refused = if ctx.viewer_ok.load(Ordering::SeqCst) {
                    crate::files::channel::refused(&ctx.gate, &who)
                } else {
                    Some("this device has not been let in yet".into())
                };
                let reply = match refused {
                    Some(err) => {
                        info!(device = %who.name, "files: refused — {err}");
                        RelayMsg::FilesAnswer {
                            sdp: String::new(),
                            err,
                        }
                    }
                    None => {
                        match crate::files::channel::open(&ctx.api, &sdp, who, ctx.gate.clone())
                            .await
                        {
                            Ok((pc, sdp)) => {
                                close_files(&ctx.files_pc);
                                *ctx.files_pc.lock().unwrap_or_else(|e| e.into_inner()) = Some(pc);
                                RelayMsg::FilesAnswer {
                                    sdp,
                                    err: String::new(),
                                }
                            }
                            Err(e) => RelayMsg::FilesAnswer {
                                sdp: String::new(),
                                err: e.to_string(),
                            },
                        }
                    }
                };
                send_relay(&out_tx, &reply).await.ok();
            }

            // No direct path for files: the same requests on the fallback lane (`files::lane`).
            RelayMsg::FilesIn { text } => {
                let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let asker = crate::files::lane::Asker {
                    who: crate::files::channel::Who { key, name },
                    gate: &ctx.gate,
                    ok: ctx.viewer_ok.load(Ordering::SeqCst),
                    out: &out_tx,
                    gauge: &lane,
                };
                files_lane.request(&text, asker).await;
            }
            RelayMsg::FilesChunk { id, b } => files_lane.chunk(id, &b).await,

            RelayMsg::SessionLaunch { command } => {
                // Answered honestly. This used to send `SessionLaunched` unconditionally, so a
                // launch into a session that does not exist was reported as a success — the
                // compositor logged `launch ignored — no active session` and the viewer was told
                // the application was starting. It then waited for a window that was never
                // coming, with the only evidence in a log it cannot see.
                if live_session(ctx).await.is_none() {
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: "there is no session to launch into — start one first".into(),
                        },
                    )
                    .await
                    .ok();
                    continue;
                }
                let _ = ctx.cmd_tx.send(CompositorCommand::Launch { command });
                send_relay(&out_tx, &RelayMsg::SessionLaunched).await.ok();
            }

            RelayMsg::ClipboardSet { text } => {
                if ctx.viewer_ok.load(Ordering::SeqCst) {
                    let clip = wado_compositor::clipboard::Clip {
                        mime: "text/plain;charset=utf-8".into(),
                        data: text.into_bytes().into(),
                        secret: false,
                    };
                    // Into the session; and onto the host too for a device that may.
                    let (device, name) =
                        ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    if ctx.gate.grants(&device).clipboard {
                        let cmd_tx = ctx.cmd_tx.clone();
                        let from = crate::clip::hub::Origin::Device { key: device, name };
                        tokio::spawn(async move {
                            let _ = crate::clip::sync::push(Some(&cmd_tx), clip, from).await;
                        });
                    } else {
                        let _ = ctx.cmd_tx.send(CompositorCommand::SetClipboard(clip));
                    }
                }
            }

            msg @ (RelayMsg::ClipList
            | RelayMsg::ClipGet { .. }
            | RelayMsg::ClipPush { .. }
            | RelayMsg::ClipPin { .. }
            | RelayMsg::ClipDelete { .. }
            | RelayMsg::ClipWatch { .. }) => {
                let (device, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if !ctx.viewer_ok.load(Ordering::SeqCst) {
                    continue;
                }
                if !ctx.gate.grants(&device).clipboard {
                    let message = "this device may not use the computer's clipboard — on the \
                                   computer: wado allow <device> clipboard"
                        .to_string();
                    send_relay(&out_tx, &RelayMsg::ClipError { message })
                        .await
                        .ok();
                    continue;
                }
                if let RelayMsg::ClipWatch { live } = msg {
                    clip_live.store(live, Ordering::SeqCst);
                    continue;
                }
                tokio::spawn(crate::clip::serve::handle(
                    msg,
                    Some(ctx.cmd_tx.clone()),
                    crate::clip::serve::Sender { key: device, name },
                    out_tx.clone(),
                ));
            }

            RelayMsg::SessionsRequest => {
                let sessions = crate::sessions::list();
                send_relay(&out_tx, &RelayMsg::Sessions { sessions })
                    .await
                    .ok();
            }

            RelayMsg::SessionDetach => {
                // Left on purpose: the media path goes, the session stays — and is kept past the
                // no-viewer grace, until someone ends it from a home page.
                // With others still in it, it simply goes on for them.
                if ctx.session_started.load(Ordering::SeqCst) {
                    let origin = ctx.seat_origin.load(Ordering::SeqCst);
                    let member = ctx.hub.is_member(origin);
                    let empty = ctx.hub.leave(origin);
                    if empty {
                        ctx.detached.store(true, Ordering::SeqCst);
                        if !member {
                            let _ = ctx.cmd_tx.send(CompositorCommand::ViewerAttached(false));
                        }
                        info!("relay client: viewer left; the session is kept running until ended");
                    } else {
                        info!("relay client: viewer left; the others in the session carry on");
                    }
                }
            }

            RelayMsg::HostGet => {
                let out_tx = out_tx.clone();
                let sent = Arc::clone(&ctx.media_sent);
                tokio::spawn(async move {
                    let mut state = crate::host::state().await;
                    sent.trim(&mut state);
                    send_relay(&out_tx, &RelayMsg::HostState { state })
                        .await
                        .ok();
                });
            }

            RelayMsg::HostDo { action } => {
                let device = ctx
                    .viewer
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .0
                    .clone();
                let allowed = if !ctx.viewer_ok.load(Ordering::SeqCst) {
                    Err("this device has not been approved yet".to_string())
                } else if action.is_network() && !ctx.gate.grants(&device).settings {
                    Err("Wi-Fi needs the settings grant — on the computer: wado allow <device> settings".to_string())
                } else if !ctx.gate.grants(&device).host {
                    Err("this device may not control the computer — on the computer: wado allow <device> host".to_string())
                } else {
                    Ok(())
                };
                let out_tx = out_tx.clone();
                let sent = Arc::clone(&ctx.media_sent);
                tokio::spawn(async move {
                    info!(device, ?action, "host action");
                    if let Err(message) = match allowed {
                        Ok(()) => crate::host::act(action).await,
                        Err(e) => Err(e),
                    } {
                        warn!("host action failed: {message}");
                        send_relay(&out_tx, &RelayMsg::HostError { message })
                            .await
                            .ok();
                    }
                    // A moment for the change to land before reading it back.
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    let mut state = crate::host::state().await;
                    sent.trim(&mut state);
                    send_relay(&out_tx, &RelayMsg::HostState { state })
                        .await
                        .ok();
                });
            }

            RelayMsg::ConfigGet => {
                let key = ctx
                    .viewer
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .0
                    .clone();
                let state = crate::config::link::state_for(&key, &ctx.gate);
                send_relay(&out_tx, &RelayMsg::ConfigState { state })
                    .await
                    .ok();
            }

            RelayMsg::ConfigSet {
                key,
                value,
                confirmed,
            } => {
                let (device, _) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let result = if ctx.viewer_ok.load(Ordering::SeqCst) {
                    crate::config::link::set(&device, &ctx.gate, &key, &value, confirmed)
                } else {
                    Err("this device has not been approved yet".into())
                };
                // Success needs no reply of its own: the reload it caused is pushed by
                // `config_task` like any other.
                if let Err(message) = result {
                    send_relay(&out_tx, &RelayMsg::ConfigRejected { key, message })
                        .await
                        .ok();
                }
            }

            RelayMsg::ConfigSetPrefs { prefs } => {
                let (device, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if ctx.viewer_ok.load(Ordering::SeqCst)
                    && let Err(e) = crate::config::link::set_prefs(&device, &name, &prefs)
                {
                    warn!("could not save device settings: {e}");
                }
            }

            RelayMsg::AppsRequest => {
                let mut apps = crate::apps::discover();
                crate::apps::running::mark(&mut apps, &ctx.cmd_tx).await;
                send_relay(&out_tx, &RelayMsg::AppsList { apps }).await.ok();
            }

            RelayMsg::PtyOpen { .. }
            | RelayMsg::PtyInput { .. }
            | RelayMsg::PtyResize { .. }
            | RelayMsg::PtyClose { .. }
            | RelayMsg::ShellsRequest
                if let Some(why) = shell_refusal(ctx, true)() =>
            {
                if crate::passkey::is_lock(why) {
                    // The device shows its lock instead of an error.
                    let key = ctx
                        .viewer
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                        .clone();
                    let state = crate::passkey::relay::state(&Default::default(), &key, "");
                    send_relay(&out_tx, &RelayMsg::PasskeyState { state })
                        .await
                        .ok();
                    continue;
                }
                if matches!(msg, RelayMsg::PtyOpen { .. } | RelayMsg::ShellsRequest) {
                    info!("shells: refused — {why}");
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: format!("no shells: {why}"),
                        },
                    )
                    .await
                    .ok();
                }
            }

            RelayMsg::PtyOpen { cols, rows, host } => match crate::shells::open(cols, rows, host) {
                Ok(id) => {
                    send_relay(&out_tx, &RelayMsg::PtyOpened { id }).await.ok();
                }
                Err(e) => {
                    warn!("shell open failed: {e}");
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: format!("could not start a shell: {e}"),
                        },
                    )
                    .await
                    .ok();
                }
            },

            RelayMsg::PtyInput { id, data } => crate::shells::write(id, &data),

            RelayMsg::PtyResize { id, cols, rows } => crate::shells::resize(id, cols, rows),

            RelayMsg::PtyClose { id } => crate::shells::close(id),

            RelayMsg::ShellsRequest => {
                let shells = crate::shells::list();
                let hosts = crate::shells::hosts::list();
                send_relay(&out_tx, &RelayMsg::Shells { shells, hosts })
                    .await
                    .ok();
                for (id, data) in crate::shells::replay() {
                    send_relay(
                        &out_tx,
                        &RelayMsg::PtyOutput {
                            id,
                            data,
                            replay: true,
                        },
                    )
                    .await
                    .ok();
                }
            }

            RelayMsg::TimingRequest => {
                let mut timings = *ctx.timings.borrow();
                timings.queue_ms = ctx.queue_us.load(Ordering::Relaxed) as f64 / 1e3;
                send_relay(&out_tx, &RelayMsg::Timing { timings })
                    .await
                    .ok();
            }

            // A task, not inline: the answer can take tens of milliseconds of D-Bus, and this
            // loop also carries the SDP and ICE of a connecting viewer.
            RelayMsg::TargetsRequest { seq, x, y, r } => {
                let cmd_tx = ctx.cmd_tx.clone();
                let a11y = Arc::clone(&ctx.a11y);
                let out_tx = out_tx.clone();
                tokio::spawn(async move {
                    let targets = crate::a11y::tap_targets(&cmd_tx, &a11y, x, y, r).await;
                    send_relay(&out_tx, &RelayMsg::Targets { seq, targets })
                        .await
                        .ok();
                });
            }

            // A row of the menu sheet. Re-read afterwards: a submenu item changes the menu in
            // place (GTK slides the submenu into the same popover), which the compositor does
            // not see as a new popup.
            RelayMsg::MenuActivate { id } => {
                let menu = ctx.menu.clone();
                let a11y = Arc::clone(&ctx.a11y);
                let out_tx = out_tx.clone();
                tokio::spawn(async move {
                    let spot = menu.borrow().clone();
                    let Some(spot) = spot else {
                        return;
                    };
                    let ok = crate::menu_sheet::activate(&a11y, &spot, &id).await;
                    debug!(ok, "menu item activated");
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    // Bound first: a borrow guard in an `if let` would live across the await.
                    let spot = menu.borrow().clone();
                    if let Some(spot) = spot {
                        let sheet = crate::menu_sheet::read(&a11y, &spot).await;
                        send_relay(&out_tx, &RelayMsg::Menu { menu: Some(sheet) })
                            .await
                            .ok();
                    }
                });
            }

            RelayMsg::SessionWindow { action } => {
                let _ = ctx.cmd_tx.send(CompositorCommand::Window(action));
                send_relay(&out_tx, &RelayMsg::SessionWindowed).await.ok();
            }

            // An offer into a running session from a seat viewer not (or no longer) in it — a
            // reconnect, an older client — is a quiet rejoin: it displaces nobody.
            RelayMsg::SdpOffer { sdp }
                if ctx.session_started.load(Ordering::SeqCst)
                    && offer_wants_video(&sdp)
                    && !ctx.hub.is_member(ctx.seat_origin.load(Ordering::SeqCst))
                    && let Err(message) =
                        ctx.hub.join(ctx.seat_origin.load(Ordering::SeqCst), false) =>
            {
                send_relay(&out_tx, &RelayMsg::SessionError { message })
                    .await
                    .ok();
            }
            RelayMsg::SdpOffer { sdp } => match handle_sdp_offer(ctx, sdp, out_tx.clone()).await {
                Ok(()) => {}
                Err(e) => {
                    error!("relay client: SdpOffer handling failed: {e}");
                    send_relay(
                        &out_tx,
                        &RelayMsg::SessionError {
                            message: format!("WebRTC setup failed: {e}"),
                        },
                    )
                    .await
                    .ok();
                }
            },

            RelayMsg::IceCandidate { candidate } => {
                let pc = ctx
                    .active_pc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if let Some(pc) = pc
                    && let Ok(cand) = serde_json::from_str(&candidate)
                {
                    let _ = pc.add_ice_candidate(cand).await;
                }
            }

            RelayMsg::ClientLog { line } => {
                info!("browser: {line}");
            }

            // How the device reaches us, every few seconds — kept for `wado tui`, logged on change.
            RelayMsg::DeviceStatus { link } => {
                if ctx.viewer_ok.load(Ordering::SeqCst) {
                    let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    ctx.hub.set_link(
                        ctx.seat_origin.load(Ordering::SeqCst),
                        crate::links::describe(&link),
                    );
                    if let Some(line) = crate::links::report(&ctx.remote_id, &key, &name, link) {
                        info!("link: {line}");
                    }
                }
            }

            RelayMsg::HostNetRequest => {
                if ctx.viewer_ok.load(Ordering::SeqCst) {
                    let net = crate::nat::host_net();
                    send_relay(&out_tx, &RelayMsg::HostNet { net }).await.ok();
                }
            }

            msg @ (RelayMsg::PasskeyBegin { .. }
            | RelayMsg::PasskeyRegister { .. }
            | RelayMsg::PasskeyUnlock { .. }
            | RelayMsg::PasskeyLock
            | RelayMsg::PasskeyKeepalive
            | RelayMsg::PasskeyGet
            | RelayMsg::PasskeyStay { .. }) => {
                let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let dev = crate::passkey::relay::Device {
                    key: &key,
                    name: &name,
                    ok: ctx.viewer_ok.load(Ordering::SeqCst),
                };
                crate::passkey::relay::handle(msg, dev, &out_tx).await;
            }

            msg @ (RelayMsg::ClaudeGet
            | RelayMsg::ClaudeAnswer { .. }
            | RelayMsg::ClaudeOpen { .. }
            | RelayMsg::ClaudeNew { .. }
            | RelayMsg::ClaudeDirsGet { .. }) => {
                let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let (ok, gate, out_tx) = (
                    ctx.viewer_ok.load(Ordering::SeqCst),
                    ctx.gate.clone(),
                    out_tx.clone(),
                );
                // A task: a new session runs tmux, and a folder listing reads the disk.
                tokio::spawn(async move {
                    let dev = crate::claude::relay::Device {
                        key: &key,
                        name: &name,
                        ok,
                        gate: &gate,
                    };
                    crate::claude::relay::handle(msg, dev, &out_tx).await;
                });
            }

            msg @ (RelayMsg::PushGet
            | RelayMsg::PushSubscribe { .. }
            | RelayMsg::PushKindsSet { .. }
            | RelayMsg::PushUnsubscribe
            | RelayMsg::PushTest) => {
                let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let (ok, out_tx) = (ctx.viewer_ok.load(Ordering::SeqCst), out_tx.clone());
                // A task: a test push waits on the push service.
                tokio::spawn(async move {
                    let dev = crate::push::relay::Device {
                        key: &key,
                        name: &name,
                        ok,
                    };
                    crate::push::relay::handle(msg, dev, &out_tx).await;
                });
            }

            RelayMsg::Ping => {
                send_relay(&out_tx, &RelayMsg::Pong).await.ok();
            }

            other => {
                warn!("relay client: unexpected message: {other:?}");
            }
        }
    }

    log_task.abort();
    text_input_task.abort();
    shedding_task.abort();
    sent_kbps_task.abort();
    windows_task.abort();
    config_task.abort();
    shells_task.abort();
    claude_task.abort();
    clipboard_task.abort();
    clip_task.abort();
    notify_task.abort();
    files_note_task.abort();
    close_files(&ctx.files_pc);
    files_lane.close();
    ctx.pad.close(); // and the trackpad: no key stays held after the device leaves
    crate::passkey::unlock::lock_link();
    menu_task.abort();
    ctl_rooms.clear(&ctl);
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

/// The offer (its JSON, as it arrives) asks for the picture — not a sound-only "play on this
/// phone" link, which is no participant.
fn offer_wants_video(offer_json: &str) -> bool {
    offer_json.contains("m=video")
}

/// The seat's viewer's offer: its peer connection is a participant's like any other
/// ([`crate::viewers::pc`]), in the seat's own slot.
async fn handle_sdp_offer(
    ctx: &RelayCtx,
    offer_json: String,
    out_tx: mpsc::Sender<String>,
) -> crate::Result<()> {
    // Which device this whole negotiation belongs to, so a log with two phones in it can be
    // read per device instead of per daemon.
    let peer = ctx.peer.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let origin = ctx.seat_origin.load(Ordering::SeqCst);
    let slot = crate::viewers::pc::Slot {
        pc: Arc::clone(&ctx.active_pc),
        generation: Arc::clone(&ctx.generation),
    };
    let (answer, trickle) = crate::viewers::pc::answer(
        &ctx.hub,
        origin,
        &offer_json,
        &slot,
        peer,
        Some(out_tx.clone()),
    )
    .await?;
    send_relay(&out_tx, &RelayMsg::SdpAnswer { sdp: answer }).await?;
    trickle.answered();
    Ok(())
}

/// The device on the relay link, for the trackpad's per-message check.
fn pad_peer(ctx: &RelayCtx) -> crate::pad::Peer {
    let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
    crate::pad::Peer {
        who: crate::files::channel::Who { key, name },
        gate: ctx.gate.clone(),
        approved: Arc::clone(&ctx.viewer_ok),
    }
}

/// Close the files peer connection, if there is one. Closed, not dropped: webrtc-rs frees its ICE
/// sockets only on `close()` (see the session's peer connection).
fn close_files(slot: &Mutex<Option<Arc<RTCPeerConnection>>>) {
    if let Some(pc) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
        tokio::spawn(async move {
            let _ = pc.close().await;
        });
    }
}

/// The session the compositor is actually running, if any.
///
/// Asked of the compositor rather than read off `ctx.session_started`: that flag belongs to one
/// relay connection and a reconnecting viewer gets a fresh one set to `false`, so it cannot answer
/// "did a session survive my disconnect?" — which is the entire question here.
async fn live_session(ctx: &RelayCtx) -> Option<wado_protocol::SessionInfo> {
    let (tx, rx) = oneshot::channel();
    ctx.cmd_tx
        .send(CompositorCommand::Status { reply: tx })
        .ok()?;
    // Bounded like every other compositor round-trip here: a wedged render loop must not hold the
    // relay socket open waiting for an answer that is not coming.
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .ok()?
        .ok()
        .flatten()
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

/// One device asking to join, as the relay described it.
struct Join {
    room_id: String,
    key: String,
    name: String,
    addr: String,
    /// Non-empty when the device says it holds a pairing code (the relay's `pair`). Only a
    /// claim: the code itself is proven inside the envelope, never sent.
    pair: String,
}

/// The device's `e2e_finish`: check its proof, then let `Gate::admit` decide on the key it
/// proved — not on the id the relay named.
async fn e2e_finish(
    ctx: &RelayCtx,
    out_tx: &mpsc::Sender<String>,
    dev_pk: &str,
    sig: &str,
    pair_mac: &str,
    typed_mac: &str,
) {
    let codes = crate::e2e::handshake::Codes {
        qr: ctx.gate.live_pairs(),
        typed: ctx.gate.live_typed(),
    };
    let (key, name) = ctx.viewer.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let done = ctx
        .link
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .finish(dev_pk, sig, pair_mac, typed_mac, &codes);
    let verdict = done.and_then(|d| {
        ctx.gate
            .admit(&key, &name, dev_pk, &d.proof)
            .map(|a| (a, d))
    });
    let mut link = ctx.link.lock().unwrap_or_else(|e| e.into_inner());
    let out = match verdict {
        Ok((crate::gate::Admitted::In, d)) => {
            info!("e2e: {name} proved its key — the link is end-to-end sealed");
            link.open(d.sealer, d.opener);
            crate::e2e::link::WAKE.to_string()
        }
        // A typed code: not in until the check words are confirmed on the computer.
        Ok((crate::gate::Admitted::Confirm(grants), d)) => {
            let crate::gate::Proof::Typed(typed) = d.proof.clone() else {
                unreachable!("only a typed code asks for the check words")
            };
            let p = crate::e2e::check::Pending {
                key,
                name,
                pk: dev_pk.to_string(),
                grants,
                typed,
            };
            let out = crate::e2e::check::begin(&mut link, &ctx.gate, &p, d);
            tokio::spawn(crate::e2e::check::settle(
                ctx.gate.clone(),
                Arc::clone(&ctx.link),
                Arc::clone(&ctx.room),
                link.room().to_string(),
                out_tx.clone(),
                p,
            ));
            out
        }
        Err(why) => {
            warn!("e2e: refused {name} — {why}");
            serde_json::to_string(&link.fail(why)).unwrap_or_default()
        }
    };
    drop(link);
    out_tx.send(out).await.ok();
}

/// Answer the relay's question about one join (or takeover check), at once: a trusted device,
/// or one that says it holds a pairing code, goes on to the envelope to prove it; anything else
/// is refused with how to pair. `on_accept` runs before the relay is told yes.
async fn decide_join(
    gate: crate::gate::Gate,
    out_tx: mpsc::Sender<String>,
    j: Join,
    takeover: bool,
    on_accept: impl FnOnce(&str) + Send + 'static,
) {
    let Join {
        room_id,
        key,
        name,
        addr,
        pair,
    } = j;
    let label = if name.is_empty() { addr } else { name };
    // Everything here is the relay's claim. It only decides whether the join is let through to
    // the envelope, where the device proves its key and `Gate::admit` decides.
    let verdict: Result<(), &str> = if gate.knows(&key) {
        Ok(())
    } else if takeover {
        // A claim alone must never displace a live viewer, code or not.
        Err(crate::gate::BUSY_UNPAIRED)
    } else if !pair.is_empty() && !key.is_empty() {
        info!("gate: {label} says it holds a pairing code — it must prove it");
        Ok(())
    } else {
        Err(crate::gate::UNPAIRED)
    };
    let msg = match verdict {
        Ok(()) => {
            on_accept(&room_id);
            info!("gate: let {label} through");
            RelayMsg::PeerAccept { room_id }
        }
        Err(reason) => {
            info!("gate: refused {label} — not paired");
            RelayMsg::PeerReject {
                room_id,
                reason: reason.into(),
            }
        }
    };
    send_relay(&out_tx, &msg).await.ok();
}

pub(crate) async fn send_relay(tx: &mpsc::Sender<String>, msg: &RelayMsg) -> crate::Result<()> {
    let text = serde_json::to_string(msg)?;
    tx.send(text)
        .await
        .map_err(|_| crate::WadoError::Other("relay out channel closed".into()))
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
///
/// Thirty minutes since `2026-10-02`, the user's choice for how long a desktop with nobody
/// connected stays open. The relay holds the viewer's seat for the same time (`hold_ms` in the
/// registration), so a seat never outlives its desktop or the other way round.
const VIEWER_GRACE: std::time::Duration = std::time::Duration::from_secs(1800);

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
    detached: Arc<AtomicBool>,
    hub: Arc<crate::viewers::Hub>,
) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tick.tick().await;
        // A session left running on purpose is kept until someone ends it.
        if !session_started.load(Ordering::SeqCst) || detached.load(Ordering::SeqCst) {
            continue;
        }
        // The lock is released before the await point; holding a std Mutex across one would
        // be a deadlock waiting for a slow tick.
        let connected = {
            let pc = active_pc.lock().unwrap_or_else(|e| e.into_inner()).clone();
            pc.map(|pc| pc.connection_state() == RTCPeerConnectionState::Connected)
                .unwrap_or(false)
                // A joined participant watching counts as much as the seat's viewer.
                || hub.any_connected()
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
        hub.end(None);
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
