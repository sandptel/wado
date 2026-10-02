//! The wado daemon's configuration — `~/.config/wado/config.kdl`, niri-style.
//!
//! The split is by job, one per module:
//!
//! - [`schema`] — what can be configured, one file per section, as plain serde structs.
//! - [`kdl`] — turning KDL text (and its `include`s) into those structs, with line:col errors.
//! - [`env`] — the `WADO_*` variables, which still win over the file.
//! - [`limits`] — the host's caps applied to what a client asks for.
//! - [`live`] — the process-wide current config, swapped whole on reload.
//! - [`paths`] — where the files live.
//!
//! Plain data on purpose: the compositor reads it, the server reads it and the WASM client
//! will share the types, so nothing here may pull in Smithay, tokio or a network type.

pub mod env;
pub mod error;
pub mod kdl;
pub mod limits;
pub mod live;
pub mod paths;
pub mod schema;

pub use error::ConfigError;
pub use schema::Config;

use std::path::Path;

/// The commented starting config written on first run. Parsed by a test, so it cannot ship
/// broken.
pub const DEFAULT_KDL: &str = include_str!("default.kdl");

/// Load a config file, its includes, and the env overrides on top.
///
/// Returns the config and the names of the env vars that overrode it, so the caller can say so
/// in the log — an env var silently beating the file is the classic "my edit does nothing".
pub fn load(path: &Path) -> Result<(Config, Vec<&'static str>), ConfigError> {
    let mut cfg = kdl::load(path)?;
    let overridden = env::overlay(&mut cfg);
    Ok((cfg, overridden))
}

/// Load the default location, writing the starting config first if there is none.
pub fn load_or_init() -> Result<(Config, Vec<&'static str>), ConfigError> {
    let path = paths::config_file();
    if !path.exists() {
        // Best effort: a read-only home still gets a working daemon on built-in defaults.
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, DEFAULT_KDL);
    }
    if !path.exists() {
        let mut cfg = Config::default();
        let overridden = env::overlay(&mut cfg);
        return Ok((cfg, overridden));
    }
    load(&path)
}
