//! Process-group lifetime for applications launched into a session.
//!
//! Killing a launched application means killing what it spawned, and a browser is the case
//! that proves it. `sh -c chromium` execs into chromium, which forks a zygote, a GPU process
//! and a renderer per tab. SIGKILL to the one pid we hold reaps that pid and nothing else:
//! every helper reparents to init and keeps running — still holding the machine's audio
//! device, still playing, with nothing left that knows how to stop them.
//!
//! So each application is spawned as its own process-group leader and signalled by group.
//! SIGTERM first, because SIGKILL cannot be caught and an application that cannot catch it
//! cannot tear down its own children either — the very thing we are trying to make happen.
//! SIGKILL follows, for whatever is still there after a short grace.
//!
//! ponytail: a process group, not a cgroup. An application that calls `setsid` for itself
//! leaves the group and escapes this — rare outside daemons, and it was escaping far more
//! before. The upgrade path when that matters is a per-session cgroup (or a transient
//! `systemd-run --scope`), which nothing can escape.

use std::{
    os::unix::process::CommandExt,
    process::{Child, Command},
    time::{Duration, Instant},
};

/// How long the group gets to leave on its own terms after SIGTERM.
///
/// Long enough for a well-behaved application to run its handler, short enough that it is
/// not felt: `stop_session` runs on the calloop thread, so this is a real pause in the
/// event loop. The render timer is already removed by the time it is spent.
const GRACE: Duration = Duration::from_millis(300);

/// How often the grace period looks to see whether the leader has gone.
const POLL: Duration = Duration::from_millis(20);

/// Spawn a command as its own process-group leader.
///
/// Through a shell, because splitting on whitespace is not what anyone means when they type
/// a command: it breaks quoted arguments into pieces and leaves `~`, `$VAR`, globs, pipes
/// and redirection as literal text. `sh -c` is the rule a desktop entry's `Exec` line
/// already follows.
pub fn spawn(command: &str) -> std::io::Result<Child> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        // The whole point: a new group, whose id is this child's pid, so everything the
        // command goes on to fork can be signalled as one unit.
        .process_group(0)
        .spawn()
}

/// Stop a launched application and everything it spawned, then reap it.
pub fn terminate(child: &mut Child) {
    // Valid only because the child has not been reaped yet: until `wait` runs it is a
    // zombie at worst, so the kernel is still holding this pid and it cannot have been
    // recycled onto somebody else's group.
    let pgid = child.id() as i32;

    signal_group(pgid, libc::SIGTERM);
    let deadline = Instant::now() + GRACE;
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }

    // Unconditional, and not a fallback for the timeout: the leader having exited says
    // nothing about the group. A shell that forks helpers and returns is exactly the shape
    // that leaves the group populated and the leader gone.
    signal_group(pgid, libc::SIGKILL);
    let _ = child.wait();
}

/// Signal a whole process group, ignoring the "already gone" answer.
fn signal_group(pgid: i32, sig: i32) {
    // Guard, not defensiveness: `killpg(0, …)` means *our own group*, which is the server,
    // the compositor and the shell that started them.
    if pgid <= 0 {
        return;
    }
    // Safe: a pid we own and a constant signal. ESRCH — nothing left in the group — is the
    // expected answer on the second call and is not worth reporting.
    unsafe {
        libc::killpg(pgid, sig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Does any process still exist in this group? Signal 0 performs the permission and
    /// existence checks without delivering anything.
    fn group_alive(pgid: i32) -> bool {
        unsafe { libc::killpg(pgid, 0) == 0 }
    }

    /// Wait for the group to empty, up to `timeout`.
    ///
    /// `terminate` guarantees the signal is delivered, not that the pids have gone: a killed
    /// process stays a zombie until reaped, and its reaper here is init, which inherits it
    /// only once the shell that owned it has died too. Asserting through that window is what
    /// makes this test flake rather than what makes it strict.
    fn wait_group_gone(pgid: i32, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if !group_alive(pgid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn kills_what_the_command_spawned_not_just_the_command() {
        // The chromium shape in miniature: the command forks helpers, and the helpers are
        // what keep running. Before this module they outlived the session and reparented to
        // init. Identified by process group, never by matching a command line — that is how
        // you kill the test runner.
        let mut child = spawn("sleep 47 & sleep 47 & wait").expect("spawn");
        let pgid = child.id() as i32;
        std::thread::sleep(Duration::from_millis(250));
        assert!(
            group_alive(pgid),
            "nothing to test — the group never started"
        );

        terminate(&mut child);
        // Two seconds against the 47 the sleeps would otherwise run for: long enough to
        // outlast reaping, far too short to pass if they were genuinely still alive.
        assert!(
            wait_group_gone(pgid, Duration::from_secs(2)),
            "helpers outlived terminate()"
        );
    }

    #[test]
    fn a_command_that_exits_on_its_own_is_still_reaped() {
        let mut child = spawn("true").expect("spawn");
        let pgid = child.id() as i32;
        terminate(&mut child);
        assert!(wait_group_gone(pgid, Duration::from_secs(2)));
    }
}
