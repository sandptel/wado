//! Where wado's files live: `$XDG_CONFIG_HOME/wado`, else `~/.config/wado`.

use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        });
    base.join("wado")
}

/// The hand-written config. wado writes it once, on first run, and never again.
pub fn config_file() -> PathBuf {
    config_dir().join("config.kdl")
}
