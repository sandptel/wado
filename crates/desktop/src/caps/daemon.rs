// SPDX-License-Identifier: AGPL-3.0-only
//! The daemon pool on this computer and the relay it dials, read from outside the daemons the
//! way `wado` and `wado tui` read it (`wado::cli::status`). Also: telling them to reload, and
//! starting one when none runs.

use std::{
    io::Write,
    os::unix::{net::UnixStream, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
};

use super::Cap;

pub struct Pool {
    /// Instances answering on their command socket.
    pub daemons: Vec<String>,
    /// The relay's `host:port`, and whether it answered.
    pub relay_addr: String,
    pub relay_up: bool,
    /// How many of this computer's daemons the relay has registered.
    pub pooled: Option<u32>,
    pub remote_id: Option<String>,
}

/// One read of the pool. May wait up to a few seconds the first time (the relay probe), so it
/// is called off the UI thread.
pub fn read() -> (Pool, Vec<wado_protocol::SessionSummary>) {
    let s = wado::cli::status::read();
    let pooled = s.pooled();
    (
        Pool {
            daemons: s.daemons,
            relay_addr: s.relay_addr,
            relay_up: s.relay.is_some(),
            pooled,
            remote_id: s.remote_id,
        },
        s.sessions,
    )
}

/// Whether any daemon answers here — the capability the console's live parts depend on.
pub fn presence(p: &Pool) -> Cap<usize> {
    if p.daemons.is_empty() {
        Cap::off(
            "no wado daemon answers on this computer",
            &["wado-msg-socket"],
        )
    } else {
        Cap::On {
            imp: p.daemons.len(),
            by: "wado-msg-socket",
        }
    }
}

/// Have every running daemon reload the config now rather than on its next poll — the same
/// nudge `wado tui` sends.
pub fn reload_all() {
    for inst in wado::cli::status::daemons() {
        if let Ok(mut s) = UnixStream::connect(wado::config::socket::path_for(&inst)) {
            let _ = s.write_all(b"reload\n");
        }
    }
}

/// This computer's name, as a person knows it.
pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "This computer".into())
}

/// The `wado` binary: on `PATH`, or beside this one.
fn wado_bin() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("wado")))
        .filter(|p| p.exists());
    beside.or_else(|| {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|d| d.join("wado"))
                .find(|p| p.exists())
        })
    })
}

/// Start a daemon in its own session (closing this app must not take it down), logging where
/// `wado tui` would. Returns at once; the next snapshot shows whether it answered.
pub fn start() -> Result<String, String> {
    let exe = wado_bin().ok_or("no `wado` binary on PATH or beside this app")?;
    let n = wado::slot::next();
    let dir = wado::cli::status::rig_dir();
    let _ = std::fs::create_dir_all(&dir);
    let log = dir.join(format!("daemon-{n}.log"));
    let out = std::fs::File::create(&log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&exe);
    cmd.arg("daemon")
        .env("WADO_INSTANCE", n.to_string())
        .env("WADO_UDP_SLICE", (n - 1).to_string())
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    // SAFETY: setsid is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc_setsid();
            Ok(())
        });
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", exe.display()))?;
    std::thread::spawn(move || child.wait());
    Ok(format!(
        "starting daemon {n} — its log is {}",
        log.display()
    ))
}

fn libc_setsid() {
    unsafe extern "C" {
        fn setsid() -> i32;
    }
    // SAFETY: no arguments, no memory; failure (already a leader) is harmless here.
    unsafe {
        setsid();
    }
}
