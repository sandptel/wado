// SPDX-License-Identifier: AGPL-3.0-only
//! This app's own appearance choices — scheme, corners, motion — kept beside its computer list
//! in `$XDG_CONFIG_HOME/wado-desktop/`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub scheme: String,
    /// 0 sharp · 1 round · 2 pill.
    pub radius: i32,
    /// 0 full · 1 reduced · 2 off.
    pub motion: i32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            scheme: crate::theme::DEFAULT.into(),
            radius: 1,
            motion: 0,
        }
    }
}

fn file() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        })
        .join("wado-desktop/appearance.json")
}

pub fn load() -> Prefs {
    std::fs::read(file())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(p: &Prefs) {
    let path = file();
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(b) = serde_json::to_vec_pretty(p) {
        let _ = std::fs::write(path, b);
    }
}
