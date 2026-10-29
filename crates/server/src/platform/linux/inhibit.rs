//! Keep the computer awake ([`PowerInhibit`] on Linux): a `systemd-inhibit` held for as long as
//! asked, so it does not suspend under a shell or a long download the phone is watching.

use std::{
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::Mutex,
};

use tokio::sync::watch;
use wado_platform::power::PowerInhibit;
use wado_platform::{Published, Res};

/// A `systemd-inhibit` child, while held.
pub struct SystemdInhibit {
    hold: Mutex<Option<Child>>,
    state: Published<bool>,
}

impl SystemdInhibit {
    pub fn new() -> SystemdInhibit {
        SystemdInhibit {
            hold: Mutex::new(None),
            state: Published::new(false),
        }
    }
}

impl PowerInhibit for SystemdInhibit {
    fn state(&self) -> watch::Receiver<bool> {
        self.state.subscribe()
    }

    fn set(&self, on: bool) -> Res<()> {
        let mut hold = self.hold.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut c) = hold.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.state.publish(false);
        if on {
            let mut cmd = Command::new("systemd-inhibit");
            cmd.args([
                "--what=idle:sleep",
                "--who=wado",
                "--why=Kept awake from a connected device",
                "sleep",
                "infinity",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
            // Ends with the daemon, however the daemon ends.
            // SAFETY: prctl is async-signal-safe and touches only this child.
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            *hold = Some(
                cmd.spawn()
                    .map_err(|e| format!("systemd-inhibit is not available ({e})"))?,
            );
            self.state.publish(true);
        }
        Ok(())
    }

    /// A hold whose process ended by itself is no hold: one non-blocking `waitpid`.
    fn check(&self) {
        let alive = self
            .hold
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().flatten().is_none());
        self.state.publish(alive);
    }
}
