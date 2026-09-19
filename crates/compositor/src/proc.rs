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
//! Resources are a second, separate concern on the same spawn. Each application also goes
//! into a transient `systemd-run --user --scope` with a CPU weight below wado's own, so that
//! a burst of work inside the session — a page loading, a video decoding — competes with the
//! render and send threads on worse terms than they do. Weights are inert until there is
//! actually contention, so this costs nothing in the common case.
//!
//! ⚠️ **The weight is a feature, not a proven fix.** The 300–500 ms pump stalls it was built
//! in response to have not yet been shown to be CPU starvation; see `runq_ms` in the pump's
//! stall log (`server/src/sched.rs`), which is the measurement that decides. If `runq_ms`
//! comes back near zero, this knob is not what fixes those stalls and must not be described
//! as though it were.
//!
//! ponytail: a scope for resources, a process group for lifetime — not one mechanism doing
//! both. An application that calls `setsid` still escapes the *group* (rare outside daemons)
//! though not the scope; unifying them means killing by cgroup, which is the upgrade path
//! when that matters.

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

/// An application launched into the session: what was asked for, and the process running it.
///
/// The command is kept because it is the only name the client and the compositor share — the
/// drawer marks a tile as running by matching this string against the desktop entry's `Exec`.
/// It is the command **as the client sent it**, before `with_ime_flag` rewrites it, or that
/// match would fail for exactly the applications the flag is added to.
pub struct Launched {
    pub command: String,
    pub child: Child,
}

/// Spawn a command as its own process-group leader.
///
/// Through a shell, because splitting on whitespace is not what anyone means when they type
/// a command: it breaks quoted arguments into pieces and leaves `~`, `$VAR`, globs, pipes
/// and redirection as literal text. `sh -c` is the rule a desktop entry's `Exec` line
/// already follows.
pub fn spawn(command: &str, env: &crate::session_env::AppEnv) -> std::io::Result<Child> {
    let mut cmd = match cpu_weight() {
        // A transient scope puts the application in its own cgroup with a CPU weight below
        // the default 100, so the kernel prefers wado's render and send threads whenever the
        // two actually compete. Below contention it changes nothing: weights only apply when
        // there is something to divide.
        Some(weight) if scope_available() => {
            let mut c = Command::new("systemd-run");
            c.args(["--user", "--scope", "--quiet", "--collect"])
                .arg(format!("--unit=wado-app-{}", unit_counter()))
                .arg(format!("--property=CPUWeight={weight}"))
                .args(["sh", "-c", command]);
            c
        }
        _ => {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        }
    };
    crate::session_env::apply(&mut cmd, env);
    // The whole point: a new group, whose id is this child's pid, so everything the
    // command goes on to fork can be signalled as one unit. Kept even under a scope —
    // the scope is for resources, the group is for lifetime, and the group is the one
    // that has been verified to reap a browser's helpers.
    cmd.process_group(0).spawn()
}

/// CPU shares for session applications, relative to the default 100 that wado itself keeps.
///
/// `WADO_APP_CPU_WEIGHT` overrides it; `0` turns the scope off entirely and spawns exactly
/// as before. The default is deliberately mild — halving an application's share under
/// contention, not starving it — because a browser inside the session is the thing the user
/// came to use, not background noise.
///
/// ponytail: one env var, no config plumbing and no UI. This is a knob whose *effect* is
/// unproven — see the note below — and a setting nobody can yet read a number for is worse
/// than no setting.
fn cpu_weight() -> Option<u32> {
    let raw =
        std::env::var("WADO_APP_CPU_WEIGHT").unwrap_or_else(|_| DEFAULT_APP_CPU_WEIGHT.to_string());
    match raw.trim().parse::<u32>() {
        Ok(0) => None,
        Ok(w) => Some(w.clamp(1, 10_000)),
        Err(_) => Some(DEFAULT_APP_CPU_WEIGHT),
    }
}

const DEFAULT_APP_CPU_WEIGHT: u32 = 50;

/// Whether a transient user scope can actually be created, probed once.
///
/// Both halves have to hold and neither can be assumed: `systemd-run` may not be installed,
/// and even where it is, the user manager may not have been delegated the `cpu` controller —
/// in which case the scope is created and the weight silently does nothing. Probing with a
/// real scope is the only answer that covers both, and it costs one `true` per process.
fn scope_available() -> bool {
    use std::sync::OnceLock;
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| {
        let ok = Command::new("systemd-run")
            .args([
                "--user",
                "--scope",
                "--quiet",
                "--collect",
                "--property=CPUWeight=100",
                "true",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            tracing::info!("no usable systemd user scope — session apps run unconstrained");
        }
        ok
    })
}

/// Unique scope names. Reusing one fails once the first is still running.
fn unit_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    std::process::id() as u64 * 1000 + N.fetch_add(1, Ordering::Relaxed)
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

    use crate::session_env::AppEnv;

    /// The pre-isolation spawn, which is what every test below is about.
    fn spawn_host(command: &str) -> std::io::Result<Child> {
        spawn(command, &AppEnv::Host { x: None })
    }

    /// `0` means "spawn exactly as before" and has to survive round-tripping, because it is
    /// the escape hatch when a scope turns out to be the thing that broke something.
    /// An unparseable value falls back to the default rather than to unconstrained: a typo
    /// in an env var should not silently disable a resource limit.
    #[test]
    fn cpu_weight_reads_the_env_and_treats_zero_as_off() {
        // ponytail: `set_var` is unsafe and this is the only test touching it, so it runs
        // without a lock. Add one if a second env-reading test ever lands here.
        unsafe {
            std::env::set_var("WADO_APP_CPU_WEIGHT", "0");
            assert_eq!(cpu_weight(), None);
            std::env::set_var("WADO_APP_CPU_WEIGHT", "25");
            assert_eq!(cpu_weight(), Some(25));
            std::env::set_var("WADO_APP_CPU_WEIGHT", "not a number");
            assert_eq!(cpu_weight(), Some(DEFAULT_APP_CPU_WEIGHT));
            std::env::remove_var("WADO_APP_CPU_WEIGHT");
            assert_eq!(cpu_weight(), Some(DEFAULT_APP_CPU_WEIGHT));
        }
    }

    /// The one that would have caught the real bug: `-p=CPUWeight=50` is not valid
    /// `systemd-run` syntax (it wants `--property=`), and the failure mode was a scope that
    /// never started at all. Asserting the weight *landed* is the only check that covers it —
    /// asserting the command ran would have passed via the fallback path.
    #[test]
    fn a_launched_app_lands_in_a_cgroup_with_the_reduced_weight() {
        if !scope_available() {
            eprintln!("no usable user scope here — skipping");
            return;
        }
        let out = std::env::temp_dir().join(format!("wado-weight-{}", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let mut child = spawn_host(&format!(
            "cat /sys/fs/cgroup$(cut -d: -f3 /proc/self/cgroup)/cpu.weight > {}",
            out.display()
        ))
        .expect("spawn");
        let _ = child.wait();

        let weight = std::fs::read_to_string(&out).unwrap_or_default();
        let _ = std::fs::remove_file(&out);
        assert_eq!(
            weight.trim().parse::<u32>().ok(),
            Some(DEFAULT_APP_CPU_WEIGHT),
            "app should run in its own scope at the reduced weight, got {weight:?}"
        );
    }

    /// The isolation is two environment edits, and getting either wrong is invisible until an
    /// application opens a window on the wrong desktop. Reading them back out of a real child
    /// is the only check that covers both.
    #[test]
    fn an_isolated_app_loses_display_and_gains_the_private_bus() {
        // ponytail: `set_var` is unsafe and process-global; this and the cpu-weight test are
        // the only two, and they touch different variables.
        unsafe { std::env::set_var("DISPLAY", ":0") };
        let out = std::env::temp_dir().join(format!("wado-env-{}", std::process::id()));
        let _ = std::fs::remove_file(&out);

        // `env`, not `printf "${DISPLAY-unset}"` — and that is not a style choice. Under the
        // systemd scope the command line is expanded by `systemd-run` *before* the shell sees
        // it, so a `${...}` in it is replaced with an empty string and the probe reads blank
        // whatever the environment actually holds. It failed exactly that way once.
        let mut child = spawn(
            &format!("env > {}", out.display()),
            &AppEnv::Isolated {
                bus: Some("unix:path=/tmp/wado-test-bus".to_string()),
                x: None,
            },
        )
        .expect("spawn");
        let _ = child.wait();

        let got = std::fs::read_to_string(&out).unwrap_or_default();
        let _ = std::fs::remove_file(&out);
        unsafe { std::env::remove_var("DISPLAY") };

        assert!(
            !got.lines().any(|l| l.starts_with("DISPLAY=")),
            "DISPLAY survived into an isolated app"
        );
        assert!(
            got.lines()
                .any(|l| l == "DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/wado-test-bus"),
            "the private bus address did not reach the app"
        );
    }

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
        let mut child = spawn_host("sleep 47 & sleep 47 & wait").expect("spawn");
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
        let mut child = spawn_host("true").expect("spawn");
        let pgid = child.id() as i32;
        terminate(&mut child);
        assert!(wait_group_gone(pgid, Duration::from_secs(2)));
    }
}
