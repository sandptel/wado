/// Spike for M-P S5: can wado ask an application "what is at this point, and how big is it?"
/// through AT-SPI, inside a session with its own private D-Bus?
///
/// Runs a portrait strip session with an isolated app, prints the session bus address, and
/// keeps the session alive so the accessibility tree can be queried from outside (gdbus).
///
/// Usage (GPU: sandbox off):
///   cargo run --release --example a11y_spike -- [command] [seconds]   # nautilus 60
use std::time::{Duration, Instant};

use wado_compositor::{
    conf::{EncoderConfig, Preset},
    headless, session_env,
    sink::file::FileSink,
};
use wado_protocol::{EncoderBackend, Placement};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "nautilus".into());
    let secs: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    std::fs::create_dir_all("captures")?;

    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (mut event_loop, mut state, _handles) = wado_compositor::build(frame_tx)?;
    state.placement = Placement::Strip;
    let ec = EncoderConfig {
        width: 720,
        height: 1600,
        fps: 30,
        bitrate_kbps: 2000,
        keyframe_interval: 60,
        preset: Preset::Ultrafast,
        backend: EncoderBackend::Software,
    };
    headless::start_session(
        &mut state,
        &ec,
        2.0,
        Box::new(FileSink::create("captures/a11y_spike.h264")?),
    )?;
    // The same isolation a real session uses with `isolate_apps`, accessibility bus included
    // (needs WADO_ATSPI — see session_env::a11y).
    state.app_bus = session_env::bus::start();
    state.app_a11y = session_env::a11y::start();
    let a11y = state.app_a11y.as_ref().map(|a| a.address().to_string());
    state.app_env = session_env::AppEnv::Isolated {
        bus: state.app_bus.as_ref().map(|b| b.address.clone()),
        a11y: a11y.clone(),
        x: None,
    };
    println!("A11Y {}", a11y.unwrap_or_default());
    std::thread::sleep(Duration::from_millis(500));
    headless::launch_command(&mut state, &command);

    let until = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < until {
        event_loop.dispatch(Some(Duration::from_millis(16)), &mut state)?;
        state.after_dispatch();
    }
    for w in state.space.elements() {
        println!(
            "WINDOW at {:?} size {:?}",
            state.space.element_location(w),
            w.geometry().size
        );
    }
    headless::stop_session(&mut state);
    Ok(())
}
