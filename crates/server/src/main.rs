use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use wado::website::{self, FRAME_CHANNEL_CAPACITY, logbus::LogBus};

/// Where the control server listens in direct mode.
const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:8080";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_bus = init_logging();

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

fn init_logging() -> LogBus {
    let log_bus = LogBus::new();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .with(log_bus.clone())
        .init();
    log_bus
}
