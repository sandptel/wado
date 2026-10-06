//! A bare `wado daemon` hosts its own way in: with no `server { relay }` configured it makes sure
//! a local `wado-relay` and a cloudflared quick tunnel in front of it are running, then dials
//! that relay like any pooled daemon. Without this a bare daemon fell back to direct mode on
//! localhost, which no phone can reach (2026-10-06).
//!
//! The relay and tunnel are started detached and shared: the next `wado daemon` finds them up and
//! joins the pool under the next free instance.
//!
//! - [`relay`] — the local relay.
//! - [`tunnel`] — the public tunnel, and the watchdog that replaces it when it dies.
//! - [`slot`] — which instance this daemon is.

pub mod relay;
pub mod slot;
pub mod tunnel;

use std::{
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Before the config loads: if no relay is configured, bring up relay and tunnel and point this
/// daemon at them through the environment (env survives config reloads; a mutated config would
/// not). Returns whether it did — the caller then starts [`tunnel::watch`].
pub fn prepare() -> bool {
    if std::env::var_os("WADO_RELAY_URL").is_some()
        || wado_config::load_or_init().is_ok_and(|l| l.config.server.relay.is_some())
    {
        return false;
    }
    let _ = std::fs::create_dir_all(crate::cli::status::rig_dir());
    if let Err(e) = relay::ensure() {
        eprintln!("  {e} — running in direct mode, reachable from this computer only");
        return false;
    }
    if let Err(e) = tunnel::ensure() {
        eprintln!("  {e} — only devices on this network can reach the relay");
    }
    let n = slot::next();
    // SAFETY: single-threaded — nothing has started yet (logging, config watcher, compositor).
    unsafe {
        std::env::set_var("WADO_RELAY_URL", format!("ws://{}", relay::ADDR));
        if std::env::var_os("WADO_INSTANCE").is_none() {
            std::env::set_var("WADO_INSTANCE", n.to_string());
            std::env::set_var("WADO_UDP_SLICE", (n - 1).to_string());
        }
    }
    true
}

/// Start `cmd` in its own session, logging to `log`: closing the terminal that started it must
/// not take down what every daemon shares.
pub fn detach(cmd: &mut Command, log: &Path) -> Result<(), String> {
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

/// `name` beside this binary, else on `PATH`.
pub fn find(name: &str) -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join(name);
    if beside.is_file() {
        return Some(beside);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}
