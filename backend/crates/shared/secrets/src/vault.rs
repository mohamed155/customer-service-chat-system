//! `Vault`: what a module holds to seal and open its secrets.
//!
//! It bundles the [`SecretBox`] (new writes, and rows already moved to envelope encryption) with
//! the earlier static key (rows not yet moved), so a call site stays one `seal` or one `open`
//! whichever kind of row it reads.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::kms::LocalKms;
use crate::legacy::{self, MasterKey};
use crate::store::PgKekStore;
use crate::{Sealed, SecretBox, SecretError};

/// The KEK that protects a tenant's secrets, or the platform's.
pub fn kek_id(tenant_id: Option<Uuid>) -> String {
    match tenant_id {
        Some(id) => format!("tenant:{id}"),
        None => "platform".to_owned(),
    }
}

/// A sealed secret as read from a row: always the ciphertext and nonce, plus the envelope
/// metadata when the row has been sealed the new way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSecret {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub envelope: Option<Envelope>,
}

/// The four metadata columns that sit next to an envelope-sealed secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub wrapped_dek: Vec<u8>,
    pub kek_id: String,
    pub kek_version: i32,
    pub algorithm: String,
}

impl StoredSecret {
    /// Build from a row's columns. The metadata columns are all present or all `NULL` (a database
    /// constraint says so); a partial set is refused rather than guessed at.
    pub fn from_columns(
        ciphertext: Vec<u8>,
        nonce: Vec<u8>,
        wrapped_dek: Option<Vec<u8>>,
        kek_id: Option<String>,
        kek_version: Option<i32>,
        algorithm: Option<String>,
    ) -> Result<Self, SecretError> {
        let envelope = match (wrapped_dek, kek_id, kek_version, algorithm) {
            (None, None, None, None) => None,
            (Some(wrapped_dek), Some(kek_id), Some(kek_version), Some(algorithm)) => {
                Some(Envelope {
                    wrapped_dek,
                    kek_id,
                    kek_version,
                    algorithm,
                })
            }
            _ => return Err(SecretError::Invalid("incomplete envelope metadata")),
        };
        Ok(Self {
            ciphertext,
            nonce,
            envelope,
        })
    }

    /// The metadata columns, to bind next to `ciphertext` and `nonce`: all `None` for an earlier
    /// secret, all `Some` for an envelope one.
    pub fn wrapped_dek(&self) -> Option<&[u8]> {
        self.envelope.as_ref().map(|e| e.wrapped_dek.as_slice())
    }

    pub fn kek_id(&self) -> Option<&str> {
        self.envelope.as_ref().map(|e| e.kek_id.as_str())
    }

    pub fn kek_version(&self) -> Option<i32> {
        self.envelope.as_ref().map(|e| e.kek_version)
    }

    pub fn algorithm(&self) -> Option<&str> {
        self.envelope.as_ref().map(|e| e.algorithm.as_str())
    }

    fn sealed(&self) -> Option<Sealed> {
        self.envelope.as_ref().map(|e| Sealed {
            ciphertext: self.ciphertext.clone(),
            nonce: self.nonce.clone(),
            wrapped_dek: e.wrapped_dek.clone(),
            kek_id: e.kek_id.clone(),
            kek_version: e.kek_version,
            algorithm: e.algorithm.clone(),
        })
    }
}

impl From<Sealed> for StoredSecret {
    fn from(sealed: Sealed) -> Self {
        Self {
            ciphertext: sealed.ciphertext,
            nonce: sealed.nonce,
            envelope: Some(Envelope {
                wrapped_dek: sealed.wrapped_dek,
                kek_id: sealed.kek_id,
                kek_version: sealed.kek_version,
                algorithm: sealed.algorithm,
            }),
        }
    }
}

pub struct Vault {
    legacy: Option<MasterKey>,
    secret_box: Option<SecretBox>,
}

impl Vault {
    pub fn new(legacy: Option<MasterKey>, secret_box: Option<SecretBox>) -> Self {
        Self { legacy, secret_box }
    }

    /// The vault of a running process: `LocalKms` holding `root_key_b64` over the `secret_keks`
    /// table, plus the earlier static key if there is one. During a root-key rotation
    /// `previous_root_key_b64` is the old root key, kept only to unwrap keys it still wraps. With
    /// no root key the vault can only open earlier rows, never seal.
    pub fn from_config(
        pool: &PgPool,
        legacy_key_b64: Option<&str>,
        root_key_b64: Option<&str>,
        previous_root_key_b64: Option<&str>,
    ) -> Result<Self, String> {
        let legacy = legacy_key_b64.map(MasterKey::from_base64).transpose()?;
        let secret_box = match root_key_b64 {
            Some(root) => {
                let root = MasterKey::from_base64(root)?;
                let mut kms = LocalKms::new(root.fingerprint(), root);
                if let Some(previous) = previous_root_key_b64 {
                    let previous = MasterKey::from_base64(previous)?;
                    kms = kms.with_previous(previous.fingerprint(), previous);
                }
                Some(SecretBox::new(
                    Arc::new(kms),
                    Arc::new(PgKekStore::new(pool.clone())),
                ))
            }
            None => None,
        };
        Ok(Self { legacy, secret_box })
    }

    /// Whether the vault holds any key at all (a process with none can neither seal nor open).
    pub fn has_keys(&self) -> bool {
        self.legacy.is_some() || self.secret_box.is_some()
    }

    /// Whether new secrets can be sealed (a root key is configured).
    pub fn can_seal(&self) -> bool {
        self.secret_box.is_some()
    }

    pub fn secret_box(&self) -> Option<&SecretBox> {
        self.secret_box.as_ref()
    }

    pub fn legacy_key(&self) -> Option<&MasterKey> {
        self.legacy.as_ref()
    }

    /// Seal under the KEK of `tenant_id` (`None` is the platform). `aad` is the scope string the
    /// ciphertext is bound to.
    pub async fn seal(
        &self,
        tenant_id: Option<Uuid>,
        aad: &str,
        plaintext: &[u8],
    ) -> Result<StoredSecret, SecretError> {
        let secret_box = self
            .secret_box
            .as_ref()
            .ok_or(SecretError::Invalid("no root key is configured"))?;
        let sealed = secret_box.seal(&kek_id(tenant_id), aad, plaintext).await?;
        Ok(sealed.into())
    }

    /// Open a stored secret, whichever way it was sealed.
    pub async fn open(
        &self,
        aad: &str,
        stored: &StoredSecret,
    ) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        match stored.sealed() {
            Some(sealed) => {
                let secret_box = self
                    .secret_box
                    .as_ref()
                    .ok_or(SecretError::Invalid("no root key is configured"))?;
                secret_box.open(aad, &sealed).await
            }
            None => {
                let key = self
                    .legacy
                    .as_ref()
                    .ok_or(SecretError::Invalid("no encryption key is configured"))?;
                legacy::open(key, aad, &stored.ciphertext, &stored.nonce)
            }
        }
    }

    /// Move an earlier secret to envelope encryption, and check the result opens to the same
    /// plaintext before returning it. `None` if it is already sealed the new way.
    pub async fn reencrypt(
        &self,
        tenant_id: Option<Uuid>,
        aad: &str,
        stored: &StoredSecret,
    ) -> Result<Option<StoredSecret>, SecretError> {
        if stored.envelope.is_some() {
            return Ok(None);
        }
        let plaintext = self.open(aad, stored).await?;
        let sealed = self.seal(tenant_id, aad, &plaintext).await?;
        let check = self.open(aad, &sealed).await?;
        if check[..] != plaintext[..] {
            return Err(SecretError::Crypto("re-encryption check failed"));
        }
        Ok(Some(sealed))
    }

    /// Wrap the data key of an envelope secret under the active KEK of its tenant. `None` when
    /// there is nothing to do (an earlier secret, or already on the active version).
    pub async fn rewrap(
        &self,
        aad: &str,
        stored: &StoredSecret,
    ) -> Result<Option<StoredSecret>, SecretError> {
        let (Some(sealed), Some(secret_box)) = (stored.sealed(), self.secret_box.as_ref()) else {
            return Ok(None);
        };
        Ok(secret_box.rewrap(aad, &sealed).await?.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryKekStore;

    fn key(fill: u8) -> MasterKey {
        MasterKey::from_bytes(&[fill; 32]).unwrap()
    }

    fn vault() -> Vault {
        let secret_box = SecretBox::new(
            Arc::new(LocalKms::new("local:v1", key(1))),
            Arc::new(InMemoryKekStore::new()),
        );
        Vault::new(Some(key(2)), Some(secret_box))
    }

    fn legacy_row(aad: &str, plaintext: &[u8]) -> StoredSecret {
        let (ciphertext, nonce) = legacy::seal(&key(2), aad, plaintext).unwrap();
        StoredSecret::from_columns(ciphertext, nonce, None, None, None, None).unwrap()
    }

    #[tokio::test]
    async fn opens_an_earlier_row_and_a_new_one() {
        let vault = vault();
        let old = legacy_row("t|openai", b"sk-old");
        let new = vault
            .seal(Some(Uuid::nil()), "t|openai", b"sk-new")
            .await
            .unwrap();
        assert!(old.envelope.is_none() && new.envelope.is_some());
        assert_eq!(&vault.open("t|openai", &old).await.unwrap()[..], b"sk-old");
        assert_eq!(&vault.open("t|openai", &new).await.unwrap()[..], b"sk-new");
    }

    #[tokio::test]
    async fn new_secrets_use_the_tenant_or_platform_key() {
        let vault = vault();
        let tenant = Uuid::now_v7();
        let a = vault.seal(Some(tenant), "s", b"x").await.unwrap();
        let b = vault.seal(None, "s", b"x").await.unwrap();
        assert_eq!(a.envelope.unwrap().kek_id, format!("tenant:{tenant}"));
        assert_eq!(b.envelope.unwrap().kek_id, "platform");
    }

    #[tokio::test]
    async fn reencrypt_moves_an_earlier_row_once_and_keeps_the_plaintext() {
        let vault = vault();
        let old = legacy_row("t|openai", b"sk-old");
        let moved = vault
            .reencrypt(Some(Uuid::nil()), "t|openai", &old)
            .await
            .unwrap()
            .expect("an earlier row moves");
        assert!(moved.envelope.is_some());
        assert_ne!(moved.ciphertext, old.ciphertext);
        assert_eq!(
            &vault.open("t|openai", &moved).await.unwrap()[..],
            b"sk-old"
        );
        // Already moved: nothing to do.
        assert!(vault
            .reencrypt(Some(Uuid::nil()), "t|openai", &moved)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn reencrypt_refuses_a_row_that_does_not_open() {
        let vault = vault();
        // Sealed for another scope: opening with this one fails, so it is not moved.
        let old = legacy_row("other|scope", b"sk-old");
        assert!(vault
            .reencrypt(Some(Uuid::nil()), "t|openai", &old)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn rewrap_follows_a_kek_rotation() {
        let vault = vault();
        let tenant = Uuid::now_v7();
        let first = vault.seal(Some(tenant), "s", b"sk").await.unwrap();
        vault
            .secret_box()
            .unwrap()
            .rotate_kek(&kek_id(Some(tenant)))
            .await
            .unwrap();
        let moved = vault.rewrap("s", &first).await.unwrap().expect("moves");
        assert_eq!(moved.envelope.as_ref().unwrap().kek_version, 2);
        assert_eq!(moved.ciphertext, first.ciphertext);
        assert_eq!(&vault.open("s", &moved).await.unwrap()[..], b"sk");
        // An earlier row has no data key to move.
        assert!(vault
            .rewrap("s", &legacy_row("s", b"x"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn without_keys_it_refuses_instead_of_guessing() {
        let none = Vault::new(None, None);
        assert!(!none.can_seal());
        assert!(none.seal(None, "s", b"x").await.is_err());
        assert!(none.open("s", &legacy_row("s", b"x")).await.is_err());
        let sealed = vault().seal(None, "s", b"x").await.unwrap();
        assert!(none.open("s", &sealed).await.is_err());
    }

    #[test]
    fn partial_envelope_metadata_is_refused() {
        let partial = StoredSecret::from_columns(
            vec![1],
            vec![2],
            Some(vec![3]),
            None,
            Some(1),
            Some("aes-256-gcm".into()),
        );
        assert_eq!(
            partial.unwrap_err(),
            SecretError::Invalid("incomplete envelope metadata")
        );
    }
}
