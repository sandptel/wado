//! `WadoCore`: the client core for Kotlin. Commands return at once (an op id where the outcome
//! comes later); state is pulled with `next_event`, a `suspend fun` in Kotlin. The core never
//! calls into the JVM from its own threads.

use std::path::PathBuf;
use std::sync::Arc;

use wado_client_core::core::Core;

use super::event::{self, ComputerInfo, CoreEvent};
use super::secret::{Adapter, SecretBox};

#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    Start { why: String },
}

impl std::error::Error for CoreError {}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let CoreError::Start { why } = self;
        f.write_str(why)
    }
}

#[derive(uniffi::Object)]
pub struct WadoCore {
    core: Core,
}

#[uniffi::export]
impl WadoCore {
    /// The core, keeping its files in `dir` (the app's private files dir).
    #[uniffi::constructor]
    pub fn new(
        dir: String,
        secret_box: Arc<dyn SecretBox>,
        device_name: String,
    ) -> Result<Arc<Self>, CoreError> {
        let core = Core::new(
            PathBuf::from(dir),
            Arc::new(Adapter(secret_box)),
            device_name,
        )
        .map_err(|why| CoreError::Start { why })?;
        Ok(Arc::new(WadoCore { core }))
    }

    pub fn client_key(&self) -> String {
        self.core.client_key()
    }

    pub fn computers(&self) -> Vec<ComputerInfo> {
        self.core
            .computers()
            .into_iter()
            .map(event::computer)
            .collect()
    }

    /// Hold a control room for exactly these computers (the ones on screen).
    pub fn watch(&self, keys: Vec<String>) {
        self.core.watch(keys)
    }

    pub fn pair(&self, link: String) -> u64 {
        self.core.pair(link)
    }

    pub fn pair_typed(&self, relay: String, remote_id: String, code: String) -> u64 {
        self.core.pair_typed(relay, remote_id, code)
    }

    pub fn forget(&self, key: String) -> u64 {
        self.core.forget(key)
    }

    pub fn shutdown(&self) {
        self.core.shutdown()
    }

    /// The next event, waiting for one.
    pub async fn next_event(&self) -> CoreEvent {
        event::event(self.core.next_event().await)
    }
}

/// Every OS logo slug the core can name: one drawable each in the app (`OsLogoTest`).
#[uniffi::export]
pub fn os_logo_slugs() -> Vec<String> {
    wado_client_core::os_logo::slugs()
        .into_iter()
        .map(str::to_string)
        .collect()
}
