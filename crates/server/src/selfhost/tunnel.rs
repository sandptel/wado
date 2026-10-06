//! The public way in: a cloudflared quick tunnel in front of the local relay, its URL read back
//! from its log (`status::tunnel`) by the QR code and the panel.
//!
//! Liveness comes from cloudflared's own `/ready` on a local metrics port: 200 while it holds a
//! connection to the edge. A quick tunnel can die under a cloudflared that keeps running —
//! Cloudflare drops it after a long disconnect and the process logs "Tunnel not found" forever,
//! with the relay and daemons all healthy behind it (2026-10-06). [`watch`] replaces it.

use std::{
    fs::File,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use crate::cli::status;

const METRICS: &str = "127.0.0.1:4001";
const CHECK: Duration = Duration::from_secs(60);
/// Consecutive failed checks before a restart. Restarting rotates the URL every phone holds, so
/// a network blip must not cause one.
const STRIKES: u32 = 3;

fn ready() -> bool {
    status::get(METRICS, "/ready").is_some_and(|(code, _)| code == 200)
}

/// Live already, or (re)started now and waited for.
pub fn ensure() -> Result<(), String> {
    if ready() {
        return Ok(());
    }
    restart()
}

fn restart() -> Result<(), String> {
    let bin = cloudflared().ok_or("cloudflared not found on PATH or in /nix/store")?;
    // ponytail: by exact name, as scripts/rig.sh does — one cloudflared per computer. A pid file
    // is the upgrade if this ever shares a machine with another tunnel.
    let _ = Command::new("pkill").args(["-x", "cloudflared"]).status();
    let log = status::rig_dir().join("tunnel.log");
    // http2 + IPv4: the defaults (QUIC, dual-stack) fail on links that block UDP/443 or advertise
    // IPv6 without a working route, and that failure looks like a hung tunnel, not an error.
    super::detach(
        Command::new(bin).args([
            "tunnel",
            "--url",
            &format!("http://{}", super::relay::ADDR),
            "--protocol",
            "http2",
            "--edge-ip-version",
            "4",
            "--metrics",
            METRICS,
        ]),
        &log,
    )?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if ready() && status::tunnel().is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "the tunnel did not come up — see {}",
        log.display()
    ))
}

fn cloudflared() -> Option<PathBuf> {
    super::find("cloudflared").or_else(|| {
        // Not on PATH in the dev shell; the newest one in the store will do.
        let mut found: Vec<PathBuf> = std::fs::read_dir("/nix/store")
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("-cloudflared-"))
            .map(|e| e.path().join("bin/cloudflared"))
            .filter(|p| p.is_file())
            .collect();
        found.sort();
        found.pop()
    })
}

/// Check the tunnel every minute for the life of the daemon and replace it after [`STRIKES`]
/// failed checks. Only the daemon holding the lock heals, so a pool does not restart it N times.
pub fn watch() {
    std::thread::Builder::new()
        .name("tunnel-watch".into())
        .spawn(|| {
            let Ok(lock) = File::create(status::rig_dir().join("tunnel.lock")) else {
                return;
            };
            let mut strikes = 0;
            loop {
                std::thread::sleep(CHECK);
                if lock.try_lock().is_err() {
                    continue;
                }
                strikes = if ready() { 0 } else { strikes + 1 };
                if strikes >= STRIKES {
                    tracing::warn!("tunnel dead for {strikes} checks — starting a new one");
                    match restart() {
                        Ok(()) => tracing::warn!(
                            url = status::tunnel().unwrap_or_default(),
                            "new tunnel up — its URL changed; rescan the QR in `wado tui`"
                        ),
                        Err(e) => tracing::error!("{e}"),
                    }
                    strikes = 0;
                }
            }
        })
        .ok();
}
