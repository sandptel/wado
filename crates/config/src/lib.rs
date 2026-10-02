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
//! - [`tier`] — which keys a client may change, and which need the owner device.
//! - [`ui_file`] — writing the client's edits to `ui.kdl`.
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
pub mod tier;
pub mod ui_file;

pub use error::ConfigError;
pub use schema::Config;

use std::path::{Path, PathBuf};

/// The commented starting config written on first run. Parsed by a test, so it cannot ship
/// broken.
pub const DEFAULT_KDL: &str = include_str!("default.kdl");

/// A loaded config, and what went into it.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: Config,
    /// Env vars that overrode the file — logged, because an env var silently beating the file
    /// is the classic "my edit does nothing".
    pub env: Vec<&'static str>,
    /// Every file read, which is what a watcher has to watch.
    pub files: Vec<PathBuf>,
}

/// Load a config file, its includes, `ui.kdl`, and the env overrides on top.
pub fn load(path: &Path) -> Result<Loaded, ConfigError> {
    let (mut config, files) = kdl::load_tracked(path)?;
    let env = env::overlay(&mut config);
    Ok(Loaded { config, env, files })
}

/// Load the default location, writing the starting config first if there is none.
pub fn load_or_init() -> Result<Loaded, ConfigError> {
    let path = paths::config_file();
    if !path.exists() {
        // Best effort: a read-only home still gets a working daemon on built-in defaults.
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, DEFAULT_KDL);
    }
    if !path.exists() {
        let mut config = Config::default();
        let env = env::overlay(&mut config);
        return Ok(Loaded {
            config,
            env,
            files: Vec::new(),
        });
    }
    load(&path)
}
