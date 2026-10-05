//! Start the rig — relay, tunnel, daemon pool — when the panel opens on a computer where none
//! of it is running. `scripts/rig.sh` knows how (setsid, ports, slices, tunnel reuse); this only
//! finds it and waits for the daemons to answer.

use std::{path::PathBuf, process::Command, time::Duration};

/// The checkout this binary was built in: `<repo>/target/release/wado`.
fn repo() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let root = exe.ancestors().nth(3)?.to_path_buf();
    root.join("scripts/rig.sh").is_file().then_some(root)
}

/// Make sure at least one daemon is up, starting the rig if none is.
pub fn ensure() -> Result<(), String> {
    if !crate::cli::status::read().daemons.is_empty() {
        return Ok(());
    }
    let root = repo().ok_or(
        "no daemon is running, and this wado was not built in a checkout with scripts/rig.sh \
         — start one with `wado daemon`",
    )?;
    println!("  no daemon running — starting the rig (relay, tunnel, daemons)…");
    let log = crate::cli::status::rig_dir();
    let _ = std::fs::create_dir_all(&log);
    let out = std::fs::File::create(log.join("rig.log")).map_err(|e| e.to_string())?;
    let status = Command::new(root.join("scripts/rig.sh"))
        .arg("--headless")
        .current_dir(&root)
        .stdout(out.try_clone().map_err(|e| e.to_string())?)
        .stderr(out)
        .status()
        .map_err(|e| format!("could not run scripts/rig.sh: {e}"))?;
    if !status.success() {
        return Err(format!(
            "the rig did not start — see {}",
            log.join("rig.log").display()
        ));
    }
    for _ in 0..40 {
        if !crate::cli::status::read().daemons.is_empty() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "the rig started but no daemon answers — see {}",
        log.display()
    ))
}
