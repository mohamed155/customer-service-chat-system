//! The key-management port: custody of the root key that wraps every key-encryption key (KEK).
//!
//! [`LocalKms`] holds the root key in process memory (development, CI and single-host installs,
//! where the key comes from the environment). A cloud KMS (AWS KMS, GCP KMS, Vault Transit) is
//! another implementation of [`Kms`]: it must keep the root key inside the service and expose
//! only `wrap` and `unwrap`, so a database leak plus an application leak is still not enough.

use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::legacy::{open_bytes, seal_bytes, MasterKey};
use crate::SecretError;

const NONCE_LEN: usize = 12;

/// A key wrapped by the root key: the id of the root key used, and `nonce || ciphertext`.
#[derive(Clone, PartialEq, Eq)]
pub struct WrappedKey {
    pub root_key_id: String,
    pub blob: Vec<u8>,
}

impl std::fmt::Debug for WrappedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WrappedKey")
            .field("root_key_id", &self.root_key_id)
            .field("blob", &"[REDACTED]")
            .finish()
    }
}

#[async_trait]
pub trait Kms: Send + Sync {
    /// The id of the root key that new wraps use.
    fn current_key_id(&self) -> &str;

    /// Wrap a 32-byte key, bound to `aad`.
    async fn wrap(&self, key: &[u8; 32], aad: &[u8]) -> Result<WrappedKey, SecretError>;

    /// Unwrap a key wrapped under any root key this service still holds.
    async fn unwrap(
        &self,
        wrapped: &WrappedKey,
        aad: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, SecretError>;
}

/// Root keys held in memory. `current` wraps; `previous` keys only unwrap, so KEKs wrapped before
/// a root-key rotation stay readable until `SecretBox::rewrap_keks` has moved them.
pub struct LocalKms {
    current: (String, MasterKey),
    previous: Vec<(String, MasterKey)>,
}

impl LocalKms {
    pub fn new(key_id: impl Into<String>, key: MasterKey) -> Self {
        Self {
            current: (key_id.into(), key),
            previous: Vec::new(),
        }
    }

    /// Keep an older root key for unwrapping only.
    pub fn with_previous(mut self, key_id: impl Into<String>, key: MasterKey) -> Self {
        self.previous.push((key_id.into(), key));
        self
    }

    fn key_for(&self, key_id: &str) -> Option<&MasterKey> {
        if self.current.0 == key_id {
            return Some(&self.current.1);
        }
        self.previous
            .iter()
            .find(|(id, _)| id == key_id)
            .map(|(_, key)| key)
    }
}

impl std::fmt::Debug for LocalKms {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalKms")
            .field("current", &self.current.0)
            .field("previous", &self.previous.len())
            .finish()
    }
}

#[async_trait]
impl Kms for LocalKms {
    fn current_key_id(&self) -> &str {
        &self.current.0
    }

    async fn wrap(&self, key: &[u8; 32], aad: &[u8]) -> Result<WrappedKey, SecretError> {
        let (ciphertext, nonce) = seal_bytes(self.current.1.as_bytes(), aad, key)?;
        let mut blob = nonce;
        blob.extend_from_slice(&ciphertext);
        Ok(WrappedKey {
            root_key_id: self.current.0.clone(),
            blob,
        })
    }

    async fn unwrap(
        &self,
        wrapped: &WrappedKey,
        aad: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, SecretError> {
        let key = self
            .key_for(&wrapped.root_key_id)
            .ok_or_else(|| SecretError::Kms("the root key for this key is not available".into()))?;
        if wrapped.blob.len() <= NONCE_LEN {
            return Err(SecretError::Invalid("wrapped key length"));
        }
        let (nonce, ciphertext) = wrapped.blob.split_at(NONCE_LEN);
        let plain = open_bytes(key.as_bytes(), aad, ciphertext, nonce)?;
        let mut out = Zeroizing::new([0u8; 32]);
        if plain.len() != 32 {
            return Err(SecretError::Invalid("unwrapped key length"));
        }
        out.copy_from_slice(&plain);
        Ok(out)
    }
}
