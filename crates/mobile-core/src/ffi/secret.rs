//! The phone's keystore, implemented in Kotlin (`KeystoreSecretBox.kt`) and handed to the core.

use std::sync::Arc;

#[derive(Debug, uniffi::Error)]
pub enum SecretError {
    Failed { why: String },
}

impl std::error::Error for SecretError {}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let SecretError::Failed { why } = self;
        f.write_str(why)
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for SecretError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        SecretError::Failed { why: e.reason }
    }
}

/// Wrap and unwrap a secret with a key the core never sees.
#[uniffi::export(with_foreign)]
pub trait SecretBox: Send + Sync {
    fn wrap(&self, plain: Vec<u8>) -> Result<Vec<u8>, SecretError>;
    fn unwrap(&self, sealed: Vec<u8>) -> Result<Vec<u8>, SecretError>;
}

/// The foreign box, as the core's trait.
pub struct Adapter(pub Arc<dyn SecretBox>);

impl wado_client_core::secret::SecretBox for Adapter {
    fn wrap(&self, plain: Vec<u8>) -> Result<Vec<u8>, String> {
        self.0
            .wrap(plain)
            .map_err(|SecretError::Failed { why }| why)
    }
    fn unwrap(&self, sealed: Vec<u8>) -> Result<Vec<u8>, String> {
        self.0
            .unwrap(sealed)
            .map_err(|SecretError::Failed { why }| why)
    }
}
