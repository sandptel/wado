//! The process-wide current config.
//!
//! Swapped whole, never edited in place: a reader takes an `Arc` snapshot and sees one
//! consistent config for as long as it holds it, even across a reload.
//!
//! ponytail: one `RwLock<Arc<_>>`, read on paths that run per session or per spawn, never per
//! frame. An `ArcSwap` is the upgrade if a per-frame reader ever appears.

use std::sync::{Arc, RwLock};

use crate::Config;

static CURRENT: RwLock<Option<Arc<Config>>> = RwLock::new(None);

/// The config in force. Built-in defaults until [`install`] has run, so a test or a tool that
/// never loads a file still gets sensible values.
pub fn current() -> Arc<Config> {
    CURRENT
        .read()
        .ok()
        .and_then(|c| c.clone())
        .unwrap_or_else(|| Arc::new(Config::default()))
}

pub fn install(cfg: Config) {
    if let Ok(mut c) = CURRENT.write() {
        *c = Some(Arc::new(cfg));
    }
}
