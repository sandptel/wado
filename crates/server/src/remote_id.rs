//! Remote ID resolution — the server's single identity/access token for relay mode.
//!
//! Resolution order:
//! 1. `WADO_REMOTE_ID` env var (normalized) — explicit override, never persisted.
//! 2. The persisted ID at `$XDG_CONFIG_HOME/wado/remote_id` (fallback `~/.config`).
//! 3. Generate a fresh 9-digit numeric ID, persist it to that file, and return it.
//!
//! The ID is stored and compared **normalized** (digits only); use
//! [`wado_protocol::relay::display_remote_id`] for the human `XXX-XXX-XXX` form.

use std::fs;
use std::path::PathBuf;

use rand::Rng;
use tracing::{info, warn};
use wado_protocol::relay::{display_remote_id, normalize_remote_id};

/// Resolve this server's Remote ID (see module docs for the order).
pub fn resolve() -> String {
    if let Ok(id) = std::env::var("WADO_REMOTE_ID") {
        let id = normalize_remote_id(&id);
        if !id.is_empty() {
            info!("Remote ID {} (from WADO_REMOTE_ID)", display_remote_id(&id));
            return id;
        }
    }

    let path = id_file_path();

    if let Ok(saved) = fs::read_to_string(&path) {
        let id = normalize_remote_id(saved.trim());
        if !id.is_empty() {
            info!("Remote ID {} (from {})", display_remote_id(&id), path.display());
            return id;
        }
    }

    let id = generate();
    match persist(&path, &id) {
        Ok(()) => info!("Remote ID {} (generated, saved to {})", display_remote_id(&id), path.display()),
        Err(e) => warn!(
            "Remote ID {} (generated; could NOT save to {}: {e} — a new ID will be generated next run)",
            display_remote_id(&id),
            path.display()
        ),
    }
    id
}

/// 9 random digits. The first digit may be 0 — the ID is a string token, not a number.
fn generate() -> String {
    let mut rng = rand::thread_rng();
    (0..9).map(|_| char::from(b'0' + rng.gen_range(0..10u8))).collect()
}

fn persist(path: &PathBuf, id: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, format!("{id}\n"))
}

/// `$XDG_CONFIG_HOME/wado/remote_id`, falling back to `~/.config/wado/remote_id`.
fn id_file_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            home.join(".config")
        });
    base.join("wado").join("remote_id")
}
