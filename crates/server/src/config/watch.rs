//! Hot reload: watch every file the config was read from, and reload when one changes.
//!
//! A valid file is installed whole; what reads it at use time (limits at the next start, app
//! env at the next spawn, the trust policy at the next join) picks it up with no restart. An
//! invalid file leaves the last good config in force and is reported — the error carries its
//! line and column, and every viewer is told.
//!
//! ponytail: a one-second mtime poll on a thread, not inotify. A config file changes when a
//! human saves it; a second is faster than they can look at the result. inotify is the upgrade
//! if a directory of hundreds of includes ever appears.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use wado_config::Config;

const POLL: Duration = Duration::from_secs(1);

static FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static BOOT: Mutex<Option<Arc<Config>>> = Mutex::new(None);

pub fn start(boot: Arc<Config>, files: Vec<PathBuf>) {
    *BOOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(boot);
    *FILES.lock().unwrap_or_else(|e| e.into_inner()) = files;
    let spawned = std::thread::Builder::new()
        .name("wado-config-watch".into())
        .spawn(|| {
            let mut seen = stamps();
            loop {
                std::thread::sleep(POLL);
                let now = stamps();
                if now != seen {
                    // Re-stamped after the reload: the set of files may have changed with it.
                    let _ = reload();
                    seen = stamps();
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("config watcher did not start ({e}); edits need a restart");
    }
}

/// Reload now. Returns the error text when the file is invalid.
pub fn reload() -> Result<(), String> {
    let path = wado_config::paths::config_file();
    // `ui.kdl` is watched even before it exists, so the client's first write is picked up.
    let mut files = vec![path.clone(), wado_config::paths::ui_file()];
    let result = match wado_config::load(&path) {
        Ok(l) => {
            let restart = needs_restart(&l.config);
            files.extend(l.files);
            wado_config::live::install(l.config);
            if restart.is_empty() {
                tracing::info!("config reloaded");
            } else {
                tracing::warn!(
                    "config reloaded; restart the daemon to apply: {}",
                    restart.join(", ")
                );
            }
            super::publish(None, restart);
            Ok(())
        }
        Err(e) => {
            tracing::error!("config not reloaded, keeping the last good one: {e}");
            super::publish(Some(e.to_string()), super::current_status().restart);
            Err(e.to_string())
        }
    };
    files.dedup();
    *FILES.lock().unwrap_or_else(|e| e.into_inner()) = files;
    result
}

/// What was read once at startup and cannot change under a running daemon.
fn needs_restart(new: &Config) -> Vec<String> {
    let boot = BOOT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(boot) = boot.as_ref() else {
        return Vec::new();
    };
    let s = (&boot.server, &new.server);
    [
        ("server.listen", s.0.listen != s.1.listen),
        ("server.relay", s.0.relay != s.1.relay),
        ("server.instance", s.0.instance != s.1.instance),
        ("server.remote-id", s.0.remote_id != s.1.remote_id),
        ("server.udp-slice", s.0.udp_slice != s.1.udp_slice),
        ("server.turn", s.0.turn != s.1.turn),
    ]
    .into_iter()
    .filter(|(_, changed)| *changed)
    .map(|(k, _)| k.to_string())
    .collect()
}

fn stamps() -> Vec<Option<SystemTime>> {
    FILES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
        .collect()
}
