//! Keep the computer awake: a `systemd-inhibit` held for as long as asked, so it does not
//! suspend under a shell or a long download the phone is watching.

use std::{
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::Mutex,
};

static HOLD: Mutex<Option<Child>> = Mutex::new(None);

pub fn on() -> bool {
    HOLD.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .is_some_and(|c| c.try_wait().ok().flatten().is_none())
}

pub fn set(on: bool) -> Result<(), String> {
    let mut hold = HOLD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut c) = hold.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
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
    }
    Ok(())
}
