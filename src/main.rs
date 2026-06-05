use smithay::reexports::{calloop::EventLoop, wayland_server::Display};
use wado::{Wado, website};

/// Where the control server listens. Bound to localhost because the session launch
/// command is free-form (an RCE surface); LAN exposure waits on the security gate.
const DEFAULT_CONTROL_ADDR: &str = "127.0.0.1:8080";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_logging();

    let mut event_loop: EventLoop<Wado> = EventLoop::try_new()?;
    let display: Display<Wado> = Display::new()?;
    let mut state = Wado::new(&mut event_loop, display);

    // Apps spawned later (on session start) connect to this socket.
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    // Start the idle control plane only. No compositor session, encoder, or render
    // loop exists until the web client triggers one — wado sits ~idle until then.
    let control_addr =
        std::env::args().nth(1).unwrap_or_else(|| DEFAULT_CONTROL_ADDR.to_string());
    website::start(state.loop_handle.clone(), &control_addr)?;
    eprintln!("[wado] idle — open http://{control_addr} to configure and start a session");

    event_loop.run(None, &mut state, move |_| {})?;

    Ok(())
}

fn init_logging() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }
}
