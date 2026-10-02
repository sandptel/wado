//! `session { }` — what a session's applications get. Privileged: `env` and `autostart` run
//! code, which is why only the file (and, later, an owner device) may set them.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Session {
    /// cgroup CPU weight for session apps (1–10000, 0 = no scope). See `compositor::proc`.
    pub app_cpu_weight: u32,
    /// at-spi2-core install prefix; unset means no accessibility tree (`WADO_ATSPI`).
    pub atspi: Option<PathBuf>,
    /// Extra environment for every launched application.
    pub env: BTreeMap<String, String>,
    /// Commands launched when a session starts, in order.
    pub autostart: Vec<String>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            app_cpu_weight: 50,
            atspi: None,
            env: BTreeMap::new(),
            autostart: Vec::new(),
        }
    }
}
