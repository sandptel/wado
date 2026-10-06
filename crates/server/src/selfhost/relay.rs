//! The local relay every self-hosted daemon dials.

use std::{process::Command, time::Duration};

use crate::cli::status;

pub const ADDR: &str = "127.0.0.1:4000";

pub fn up() -> bool {
    status::health(ADDR).is_some()
}

/// Running already, or started now from the `wado-relay` beside this binary.
pub fn ensure() -> Result<(), String> {
    if up() {
        return Ok(());
    }
    let bin = super::find("wado-relay").ok_or("no wado-relay beside wado or on PATH")?;
    let log = status::rig_dir().join("relay.log");
    // --trust-proxy: every phone arrives through cloudflared from 127.0.0.1, and the join rate
    // limit is per address — without it they would all share one bucket.
    super::detach(
        Command::new(bin).args(["--log-level", "info", "--trust-proxy"]),
        &log,
    )?;
    for _ in 0..40 {
        if up() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "wado-relay did not come up — see {}",
        log.display()
    ))
}
