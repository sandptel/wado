use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt as _, prelude::*};
use wado::website::{self, FRAME_CHANNEL_CAPACITY, logbus::LogBus};

/// Where the control server listens in direct mode.
const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:8080";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_bus = init_logging();

    // A debug build cannot meet the latency target and does not fail in a way that looks
    // like a build problem: it looks like a network or encoder fault. SRTP encrypts and
    // authenticates every RTP packet, `write_sample` awaits once per packet, and unoptimised
    // that adds up to `write_sample` stalls of 100-200 ms, a jitter buffer climbing past
    // 40 ms, and frame drops at 1080p — all of which read as "the transport is too slow".
    // Diagnosing that from the symptoms cost hours once. It says so now instead.
    //
    // A warning, not a refusal: `cargo run` for a quick check is legitimate, and the log
    // bus carries this line to the client's log panel too.
    if cfg!(debug_assertions) {
        tracing::warn!(
            "DEBUG BUILD — unoptimised SRTP and packetisation will stall the frame pump and \
             inflate latency. Measurements taken from this build are not meaningful. \
             Use `cargo build --release` and run `./target/release/wado` for anything real."
        );
    }

    let (frame_tx, frame_rx) = tokio::sync::mpsc::channel(FRAME_CHANNEL_CAPACITY);
    let (mut event_loop, mut state, handles) = wado_compositor::build(frame_tx)?;

    // ── Mode selection ───────────────────────────────────────────────────────
    // Relay mode: set WADO_RELAY_URL (e.g. ws://my-vps:4000). The server's identity
    // is its Remote ID — resolved by `remote_id::resolve()` (WADO_REMOTE_ID env var,
    // else the persisted ~/.config/wado/remote_id, else generated + persisted).
    // Clients connect with that single ID; no separate password.
    //
    // Direct mode (default): server binds an HTTP control endpoint.
    //   First CLI argument overrides the default listen address (127.0.0.1:8080).

    if let Ok(relay_url) = std::env::var("WADO_RELAY_URL") {
        let remote_id = wado::remote_id::resolve();

        tracing::info!(
            relay_url = %relay_url,
            "wado server — relay mode"
        );

        wado::relay_client::start(
            handles.commands,
            handles.input,
            handles.timings,
            frame_rx,
            relay_url,
            remote_id,
            log_bus,
        )?;
    } else {
        let control_addr =
            std::env::args().nth(1).unwrap_or_else(|| DEFAULT_CONTROL_ADDR.to_string());

        tracing::info!(
            addr = %control_addr,
            "wado server — direct mode (http)"
        );

        website::start(
            handles.commands,
            handles.input,
            frame_rx,
            handles.timings,
            &control_addr,
            log_bus,
        )?;
        tracing::info!("wado server idle on http://{control_addr} — connect with the wado-client app");
    }

    // Post-dispatch flush: calloop runs this after EVERY dispatch, so remote input
    // synthesized on the input channel reaches the app immediately instead of waiting
    // for the next render tick to flush it (which quantised input to the frame period).
    // One flush per loop iteration, however many sources fired — cheaper than flushing
    // per event and it covers commands and Wayland traffic too.
    event_loop.run(None, &mut state, |state| state.flush_clients())?;
    Ok(())
}

/// The terminal's default verbosity.
///
/// Two things this fixes, both of which cost a debugging session. **wado's own crates default
/// to `debug`**, because the previous blanket `info` meant every `debug!` anyone added to this
/// codebase was dead in the live daemon — written, shipped, and silent, which looks exactly
/// like the thing it was watching for never happening. And **`webrtc_ice` is muted to `warn`**:
/// a single session teardown emits eight "Failed to close candidate … the agent is closed"
/// lines, none of which has ever meant anything.
///
/// Per-lane detail needs no new code — a tracing target *is* the module path, so
/// `RUST_LOG=wado_compositor::headless=debug,wado_compositor::input=trace` already works.
/// `RUST_LOG` overrides this whole string when set.
const DEFAULT_LOG: &str = "info,wado=debug,wado_compositor=debug,webrtc_ice=warn";

/// What the client's log panel sees. Deliberately **not** the string above: the panel is a
/// 200-line ring in front of a human, and wado's own debug traffic would push anything worth
/// reading off the top of it within seconds.
const PANEL_LOG: &str = "info";

fn init_logging() -> LogBus {
    let log_bus = LogBus::new();
    // Filters are per-layer, not on the registry. A registry-level filter gates every layer at
    // once, which is what made the terminal and the client's panel share one verbosity — and
    // why neither could be turned up without flooding the other.
    let term = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG));
    tracing_subscriber::registry()
        .with(fmt::layer().with_filter(term))
        .with(log_bus.clone().with_filter(EnvFilter::new(PANEL_LOG)))
        .init();
    // After the subscriber, so a panic during startup still has somewhere to go. Panics in the
    // render and command paths are already caught per session; this is for everything else —
    // notably tokio tasks, where a dead task looks exactly like a quiet one.
    wado::panic_log::install();
    log_bus
}
