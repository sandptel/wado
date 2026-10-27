//! Start a daemon when the panel opens on a computer where none is running. It dials the
//! configured relay (the public one by default); this only starts it and waits for it to answer.

use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use crate::cli::status;

/// Make sure at least one daemon is up, starting one if none is.
///
/// ponytail: one daemon, so one device at a time. Run `wado daemon` again to grow the pool.
pub fn ensure() -> Result<(), String> {
    if !status::daemons().is_empty() {
        return Ok(());
    }
    println!("  no daemon running — starting one…");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let n = crate::slot::next();
    let log = status::rig_dir().join(format!("daemon-{n}.log"));
    let _ = std::fs::create_dir_all(status::rig_dir());
    detach(
        Command::new(exe)
            .arg("daemon")
            .env("WADO_INSTANCE", n.to_string())
            .env("WADO_UDP_SLICE", (n - 1).to_string()),
        &log,
    )?;
    for _ in 0..240 {
        if !status::daemons().is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "the daemon started but does not answer — see {}",
        log.display()
    ))
}

/// Start `cmd` in its own session, logging to `log`: closing the terminal that started it must
/// not take the daemon down.
fn detach(cmd: &mut Command, log: &Path) -> Result<(), String> {
    let out = std::fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("could not start {:?}: {e}", cmd.get_program()))?;
    // Reaped if it exits while we live; reparented to init if we go first.
    std::thread::spawn(move || child.wait());
    Ok(())
}
