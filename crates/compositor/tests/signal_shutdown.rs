//! A signalled daemon must stop its session before it goes.
//!
//! The unit tests in `proc` prove a group dies when `stop_session` asks it to. This proves
//! the other half — that a SIGTERM reaches `stop_session` at all. Before the signal source
//! existed the process simply died, every launched application outlived the session that
//! owned it, and nothing in the codebase noticed.

use std::time::{Duration, Instant};

/// Wait for every process in `pgid` to be gone, up to `timeout`.
///
/// Polled rather than checked once, because `terminate` guarantees the signal is delivered,
/// not that the pids have already vanished: a killed process is a zombie until its parent —
/// here init, after the shell it belonged to died first — gets round to reaping it. That
/// window is short and real, and asserting through it is how this test flakes.
fn wait_group_gone(pgid: i32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if unsafe { libc::killpg(pgid, 0) } != 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn sigterm_stops_the_session_and_its_applications() {
    let (frame_tx, _frame_rx) = tokio::sync::mpsc::channel(2);
    let (mut event_loop, mut state, _handles) = wado_compositor::build(frame_tx).expect("build");

    // A session with one application in it, which is all `stop_session` needs to act on —
    // standing up the render pipeline would need a GPU and proves nothing extra here.
    state.session_active = true;
    let child = wado_compositor::proc::spawn("sleep 53 & sleep 53 & wait").expect("spawn");
    let pgid = child.id() as i32;
    state.app_processes.push(child);
    std::thread::sleep(Duration::from_millis(250));
    assert!(
        unsafe { libc::killpg(pgid, 0) } == 0,
        "nothing to test — the application never started"
    );

    // To this thread specifically: `raise` is a self-signal, and the signalfd calloop
    // installed is armed on the thread that built it, which is this one.
    unsafe { libc::raise(libc::SIGTERM) };

    // Dispatch until the handler calls `loop_signal.stop()`, which ends `run`.
    let started = Instant::now();
    event_loop
        .run(Some(Duration::from_millis(50)), &mut state, |_| {})
        .expect("run");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "loop did not stop on the signal"
    );

    assert!(!state.session_active, "session was left active");
    // Generous next to the 53 s the sleeps would otherwise run for: this separates "reaped a
    // moment later" from "still running", and those are two seconds apart, not milliseconds.
    assert!(
        wait_group_gone(pgid, Duration::from_secs(2)),
        "the application outlived the signalled daemon"
    );
}
