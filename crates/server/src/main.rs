use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use wado::{
    Wado,
    website::{self, logbus::LogBus},
};

/// Where the control server listens. Bound to localhost because the session launch
/// command is free-form (an RCE surface); LAN exposure waits on the security gate.
const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:8080";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_bus = init_logging();

    let mut event_loop: EventLoop<Wado> = EventLoop::try_new()?;
    let display: Display<Wado> = Display::new()?;
    let mut state = Wado::new(&mut event_loop, display);

    // Apps spawned later (on session start) connect to this socket.
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    // Start the idle control plane only. No compositor session, encoder, or render
    // loop exists until a client triggers one — wado sits ~idle until then.
    let control_addr =
        std::env::args().nth(1).unwrap_or_else(|| DEFAULT_CONTROL_ADDR.to_string());
    website::start(state.loop_handle.clone(), &control_addr, log_bus)?;
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
