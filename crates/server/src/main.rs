use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use wado::website::{self, FRAME_CHANNEL_CAPACITY, logbus::LogBus};

/// Where the control server listens. Bound to localhost because the session launch
/// command is free-form (an RCE surface); LAN exposure waits on the security gate.
const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:8080";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_bus = init_logging();

    // Encoded-frame channel: the compositor's session ChannelSink feeds frame_tx; the
    // server's WebRTC frame pump owns frame_rx. The command Sender flows the other way.
    let (frame_tx, frame_rx) = tokio::sync::mpsc::channel(FRAME_CHANNEL_CAPACITY);

    // Build the compositor (event loop + state + command Sender). `build` claims the
    // Wayland socket and exports WAYLAND_DISPLAY for apps spawned into the session.
    let (mut event_loop, mut state, cmd_tx) = wado_compositor::build(frame_tx)?;

    // Start the idle control plane only. No compositor session, encoder, or render
    // loop exists until a client triggers one — wado sits ~idle until then. The server
    // holds only the command Sender + frame Receiver; it never touches Wado/Smithay.
    let control_addr =
        std::env::args().nth(1).unwrap_or_else(|| DEFAULT_CONTROL_ADDR.to_string());
    website::start(cmd_tx, frame_rx, &control_addr, log_bus)?;
    tracing::info!("wado server idle on http://{control_addr} — connect with the wado-client app");

    event_loop.run(None, &mut state, move |_| {})?;

    Ok(())
}

/// Install tracing: the console fmt layer, plus the [`LogBus`] layer that feeds the
/// web client's live log panel. Returns the bus so it can be handed to the server.
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
