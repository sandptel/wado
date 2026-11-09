/// Live check for M-P S5's tap-target query (`wado::a11y`): a real session with its own
/// accessibility bus, a real app, and the query run for a few points — with its timing, since
/// the answer has to arrive within a tap.
///
/// Usage (GPU: sandbox off; WADO_ATSPI set, as the dev shell does):
///   cargo run --release -p wado --example targets_probe -- [command]    # nautilus
use std::time::{Duration, Instant};

use wado_compositor::{
    conf::{EncoderConfig, Preset},
    headless, session_env,
    sink::file::FileSink,
};
use wado_protocol::{EncoderBackend, Placement};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "nautilus".into());
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
        Box::new(FileSink::create("captures/targets_probe.h264")?),
    )?;
    state.app_bus = session_env::bus::start();
    state.app_a11y = session_env::a11y::start();
    state.app_env = session_env::AppEnv::Isolated {
        bus: state.app_bus.as_ref().map(|b| b.address.clone()),
        a11y: state.app_a11y.as_ref().map(|a| a.address().to_string()),
        x: None,
    };
    std::thread::sleep(Duration::from_millis(500));
    headless::launch_command(&mut state, &command);
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        event_loop.dispatch(Some(Duration::from_millis(16)), &mut state)?;
        state.after_dispatch();
    }

    let rt = tokio::runtime::Runtime::new()?;
    let a11y = wado::a11y::A11y::default();
    // Normalized points on a 360x800 logical output: header left, header right, a grid item,
    // the bottom-right button. Radius ~22 logical px (a fingertip).
    for (x, y) in [(0.05, 0.028), (0.94, 0.028), (0.25, 0.15), (0.89, 0.97)] {
        let Some(hit) = state.hit_window(x, y) else {
            println!("({x},{y}): no window / no a11y bus");
            continue;
        };
        let t = Instant::now();
        let targets = rt.block_on(a11y.targets(&hit, 22.0 / hit.factor));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        match targets {
            None => println!("({x},{y}) local {:?}: no tree  [{ms:.1} ms]", hit.local),
            Some(ts) => {
                println!(
                    "({x},{y}) local {:?}: {} target(s)  [{ms:.1} ms]",
                    hit.local,
                    ts.len()
                );
                for t in ts {
                    println!(
                        "    {} {:?} at {},{} {}x{}",
                        t.role, t.name, t.x, t.y, t.w, t.h
                    );
                }
            }
        }
    }
    // S7: right-click the first folder, then read the menu that opens.
    for pressed in [true, false] {
        state.handle_remote_input(wado_protocol::InputEvent::Button {
            x: 0.25,
            y: 0.15,
            button: wado_protocol::PointerButton::Right,
            pressed,
        });
    }
    let until = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < until {
        event_loop.dispatch(Some(Duration::from_millis(16)), &mut state)?;
        state.after_dispatch();
    }
    match state.open_menu() {
        None => println!("menu: no popup open after the right-click"),
        Some(spot) => {
            println!("menu: popup at {:?} (window-local)", spot.rect);
            let t = Instant::now();
            let sheet = rt.block_on(wado::menu_sheet::read(&a11y, &spot));
            println!(
                "menu: tree={} items={} [{:.1} ms]",
                sheet.tree,
                sheet.items.len(),
                t.elapsed().as_secs_f64() * 1e3
            );
            for i in &sheet.items {
                println!(
                    "    {:?} enabled={} checked={} submenu={}",
                    i.name, i.enabled, i.checked, i.submenu
                );
            }
            // Activate "Properties" through the tree; a dialog should appear.
            let before = state.space.elements().count();
            if let Some(item) = sheet.items.iter().find(|i| i.name == "Properties") {
                let ok = rt.block_on(wado::menu_sheet::activate(&a11y, &spot, &item.id));
                let until = Instant::now() + Duration::from_millis(2000);
                while Instant::now() < until {
                    event_loop.dispatch(Some(Duration::from_millis(16)), &mut state)?;
                    state.after_dispatch();
                }
                println!(
                    "menu: activated Properties → {ok}; windows {before} → {}; menu still open: {}",
                    state.space.elements().count(),
                    state.open_menu().is_some()
                );
                if let Some(px) = headless::snapshot_rgba(&mut state) {
                    std::fs::write("captures/targets_probe.rgba", px)?;
                }
            }
        }
    }
    headless::stop_session(&mut state);
    Ok(())
}
