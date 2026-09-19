//! What the drawer's running dot is drawn from: the session's live child processes.
//!
//! An integration test rather than a unit one because it needs a real `Wado` — the list lives
//! on the state, and the reaping happens on the way out of it.

use std::time::Duration;

use wado_compositor::headless;

#[test]
fn only_the_applications_that_are_still_alive_are_reported() {
    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (_event_loop, mut state, _handles) = wado_compositor::build(frame_tx).expect("build");

    // No render pipeline: `launch_command` spawns a process and records it, which is the whole
    // of what is under test. Standing up a GPU session would prove nothing extra.
    state.session_active = true;
    headless::launch_command(&mut state, "sleep 47");
    headless::launch_command(&mut state, "true");

    // `true` needs a moment to actually exit; without it this asserts a race, not a rule.
    std::thread::sleep(Duration::from_millis(300));

    let running = headless::running_apps(&mut state);
    assert_eq!(
        running,
        vec!["sleep 47".to_string()],
        "a command that has exited must not keep its dot"
    );

    // The command is reported as the client sent it — that string is the join key the drawer
    // matches against a desktop entry's `Exec`, so a rewritten one would silently never match.
    headless::stop_session(&mut state);
    assert!(headless::running_apps(&mut state).is_empty());
}

/// Chromium's `--enable-wayland-ime` is added on the way to the shell, and the drawer would
/// never match a tile again if that rewritten command were what came back.
#[test]
fn the_reported_command_is_the_one_that_was_asked_for() {
    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (_event_loop, mut state, _handles) = wado_compositor::build(frame_tx).expect("build");
    state.session_active = true;

    // Not chromium itself — a command named like it, which is all `with_ime_flag` looks at.
    // It exits immediately; the point is what was *recorded*, so it is read before the reap.
    headless::launch_command(&mut state, "chromium");
    let recorded: Vec<String> = state
        .app_processes
        .iter()
        .map(|a| a.command.clone())
        .collect();
    headless::stop_session(&mut state);

    assert_eq!(recorded, vec!["chromium".to_string()]);
}
