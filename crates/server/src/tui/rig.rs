//! Start a daemon when the panel opens on a computer where none is running. It brings up its own
//! relay and tunnel (`crate::selfhost`); this only starts it and waits for it to answer.

use std::{process::Command, time::Duration};

use crate::cli::status;

/// Make sure at least one daemon is up, starting one if none is.
///
/// ponytail: one daemon, so one device at a time. Run `wado daemon` again to grow the pool.
pub fn ensure() -> Result<(), String> {
    if !status::daemons().is_empty() {
        return Ok(());
    }
    println!("  no daemon running — starting one (relay and tunnel too, if needed)…");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let n = crate::selfhost::slot::next();
    let log = status::rig_dir().join(format!("daemon-{n}.log"));
    let _ = std::fs::create_dir_all(status::rig_dir());
    crate::selfhost::detach(
        Command::new(exe)
            .arg("daemon")
            .env("WADO_INSTANCE", n.to_string())
            .env("WADO_UDP_SLICE", (n - 1).to_string()),
        &log,
    )?;
    // The tunnel alone can take ~30 s on a cold start.
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
