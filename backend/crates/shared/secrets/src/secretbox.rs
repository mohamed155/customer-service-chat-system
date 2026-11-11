//! `SecretBox`: seal a secret under its own data key, wrap that key under the tenant's KEK.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::kms::{Kms, WrappedKey};
use crate::legacy::{open_bytes, seal_bytes};
use crate::store::{KekRecord, KekState, KekStore};
use crate::SecretError;

/// The cipher used for the secret and for both wraps.
pub const ALGORITHM: &str = "aes-256-gcm";

const NONCE_LEN: usize = 12;
/// How long an unwrapped KEK stays in memory. Destroying a KEK takes effect in every process
/// within this time (at once in the process that destroyed it).
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// A secret as stored: the ciphertext, its nonce, the wrapped data key, and which key wrapped it.
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    /// `nonce || ciphertext` of the data key under the KEK.
    pub wrapped_dek: Vec<u8>,
    pub kek_id: String,
    pub kek_version: i32,
    pub algorithm: String,
}

impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sealed")
            .field("kek_id", &self.kek_id)
            .field("kek_version", &self.kek_version)
            .field("algorithm", &self.algorithm)
            .finish_non_exhaustive()
    }
}

type CacheKey = (String, i32);
type CacheEntry = (Instant, Zeroizing<[u8; 32]>);

pub struct SecretBox {
    kms: Arc<dyn Kms>,
    store: Arc<dyn KekStore>,
    cache: Mutex<HashMap<CacheKey, CacheEntry>>,
    ttl: Duration,
}

fn kek_aad(kek_id: &str, version: i32) -> Vec<u8> {
    format!("kek|{kek_id}|{version}").into_bytes()
}

fn dek_aad(kek_id: &str, version: i32, aad: &str) -> Vec<u8> {
    format!("dek|{kek_id}|{version}|{aad}").into_bytes()
}

fn random_key() -> Result<Zeroizing<[u8; 32]>, SecretError> {
    use aes_gcm::aead::{Generate, Key};
    use aes_gcm::Aes256Gcm;
    let mut generated = Key::<Aes256Gcm>::generate();
    let mut key = Zeroizing::new([0u8; 32]);
    if generated.len() != key.len() {
        return Err(SecretError::Crypto("key generation"));
    }
    key.copy_from_slice(&generated);
    generated.iter_mut().for_each(|byte| *byte = 0);
    Ok(key)
}

impl SecretBox {
    pub fn new(kms: Arc<dyn Kms>, store: Arc<dyn KekStore>) -> Self {
        Self {
            kms,
            store,
            cache: Mutex::new(HashMap::new()),
            ttl: DEFAULT_CACHE_TTL,
        }
    }

    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Seal `plaintext` for `aad` (the scope string a ciphertext is bound to) under a new data
    /// key, wrapped by the active KEK of `kek_id` (created on first use).
    pub async fn seal(
        &self,
        kek_id: &str,
        aad: &str,
        plaintext: &[u8],
    ) -> Result<Sealed, SecretError> {
        let (version, kek) = self.active_kek(kek_id).await?;
        let dek = random_key()?;
        let (ciphertext, nonce) = seal_bytes(&dek, aad.as_bytes(), plaintext)?;
        let wrapped_dek = wrap_dek(&kek, &dek_aad(kek_id, version, aad), &dek)?;
        Ok(Sealed {
            ciphertext,
            nonce,
            wrapped_dek,
            kek_id: kek_id.to_owned(),
            kek_version: version,
            algorithm: ALGORITHM.to_owned(),
        })
    }

    /// Open a sealed secret. Fails if `aad` is not the scope it was sealed for, if anything was
    /// changed, or if its KEK was destroyed.
    pub async fn open(
        &self,
        aad: &str,
        sealed: &Sealed,
    ) -> Result<Zeroizing<Vec<u8>>, SecretError> {
        let dek = self.unwrap_dek(aad, sealed).await?;
        open_bytes(&dek, aad.as_bytes(), &sealed.ciphertext, &sealed.nonce)
    }

    /// Wrap the data key again under the active KEK, without touching the ciphertext.
    /// `None` when it already uses the active version.
    pub async fn rewrap(&self, aad: &str, sealed: &Sealed) -> Result<Option<Sealed>, SecretError> {
        let (version, kek) = self.active_kek(&sealed.kek_id).await?;
        if version == sealed.kek_version {
            return Ok(None);
        }
        let dek = self.unwrap_dek(aad, sealed).await?;
        let wrapped_dek = wrap_dek(&kek, &dek_aad(&sealed.kek_id, version, aad), &dek)?;
        Ok(Some(Sealed {
            wrapped_dek,
            kek_version: version,
            ..sealed.clone()
        }))
    }

    /// Start a new KEK version for `kek_id`. Secrets keep opening under the old version until
    /// they are re-wrapped. Returns the new version.
    pub async fn rotate_kek(&self, kek_id: &str) -> Result<i32, SecretError> {
        let current = match self.store.active(kek_id).await? {
            Some(record) => record,
            None => {
                self.active_kek(kek_id).await?;
                return self
                    .store
                    .active(kek_id)
                    .await?
                    .map(|r| r.version)
                    .ok_or(SecretError::KekMissing);
            }
        };
        let version = self
            .store
            .latest_version(kek_id)
            .await?
            .unwrap_or(current.version)
            + 1;
        let record = self.new_record(kek_id, version).await?;
        if !self.store.rotate(kek_id, current.version, &record).await? {
            return Err(SecretError::Store(
                "the key was rotated by someone else, try again".into(),
            ));
        }
        Ok(version)
    }

    /// Wrap every KEK that an older root key still wraps under the current root key. Returns how
    /// many were moved. Run until it returns 0, then the old root key can be retired.
    pub async fn rewrap_keks(&self, limit: i64) -> Result<u64, SecretError> {
        let stale = self
            .store
            .not_wrapped_by(self.kms.current_key_id(), limit)
            .await?;
        let mut moved = 0;
        for record in stale {
            let Some(old) = record.wrapped.as_ref() else {
                continue;
            };
            let aad = kek_aad(&record.kek_id, record.version);
            let key = self.kms.unwrap(old, &aad).await?;
            let wrapped = self.kms.wrap(&key, &aad).await?;
            if self
                .store
                .replace_wrapped(&record.kek_id, record.version, &old.root_key_id, &wrapped)
                .await?
            {
                moved += 1;
            }
        }
        Ok(moved)
    }

    /// Destroy every version of a KEK (crypto-shredding). Every secret under it is gone for good,
    /// backups included. Returns how many versions were destroyed.
    pub async fn destroy_kek(&self, kek_id: &str) -> Result<u64, SecretError> {
        let destroyed = self.store.destroy(kek_id).await?;
        self.cache.lock().await.retain(|(id, _), _| id != kek_id);
        Ok(destroyed)
    }

    async fn unwrap_dek(
        &self,
        aad: &str,
        sealed: &Sealed,
    ) -> Result<Zeroizing<[u8; 32]>, SecretError> {
        if sealed.algorithm != ALGORITHM {
            return Err(SecretError::Invalid("algorithm"));
        }
        let kek = self.kek(&sealed.kek_id, sealed.kek_version).await?;
        let aad = dek_aad(&sealed.kek_id, sealed.kek_version, aad);
        if sealed.wrapped_dek.len() <= NONCE_LEN {
            return Err(SecretError::Invalid("wrapped key length"));
        }
        let (nonce, ciphertext) = sealed.wrapped_dek.split_at(NONCE_LEN);
        let plain = open_bytes(&kek, &aad, ciphertext, nonce)?;
        if plain.len() != 32 {
            return Err(SecretError::Invalid("unwrapped key length"));
        }
        let mut dek = Zeroizing::new([0u8; 32]);
        dek.copy_from_slice(&plain);
        Ok(dek)
    }

    /// The active KEK of `kek_id`, created on first use.
    async fn active_kek(&self, kek_id: &str) -> Result<(i32, Zeroizing<[u8; 32]>), SecretError> {
        for _ in 0..3 {
            if let Some(record) = self.store.active(kek_id).await? {
                let key = self.kek_from_record(&record).await?;
                return Ok((record.version, key));
            }
            // Never reuse a destroyed KEK's id with version 1: continue after the latest.
            let version = self.store.latest_version(kek_id).await?.unwrap_or(0) + 1;
            let record = self.new_record(kek_id, version).await?;
            // A lost race (another process created it first) is fine: load the winner.
            self.store.insert_active(&record).await?;
        }
        Err(SecretError::Store("could not create the key".into()))
    }

    async fn new_record(&self, kek_id: &str, version: i32) -> Result<KekRecord, SecretError> {
        let key = random_key()?;
        let wrapped: WrappedKey = self.kms.wrap(&key, &kek_aad(kek_id, version)).await?;
        Ok(KekRecord {
            kek_id: kek_id.to_owned(),
            version,
            state: KekState::Active,
            wrapped: Some(wrapped),
        })
    }

    async fn kek(&self, kek_id: &str, version: i32) -> Result<Zeroizing<[u8; 32]>, SecretError> {
        let cache_key = (kek_id.to_owned(), version);
        if let Some((at, key)) = self.cache.lock().await.get(&cache_key) {
            if at.elapsed() < self.ttl {
                return Ok(key.clone());
            }
        }
        let record = self
            .store
            .get(kek_id, version)
            .await?
            .ok_or(SecretError::KekMissing)?;
        self.kek_from_record(&record).await
    }

    async fn kek_from_record(
        &self,
        record: &KekRecord,
    ) -> Result<Zeroizing<[u8; 32]>, SecretError> {
        if record.state == KekState::Destroyed {
            return Err(SecretError::KekDestroyed);
        }
        let cache_key = (record.kek_id.clone(), record.version);
        if let Some((at, key)) = self.cache.lock().await.get(&cache_key) {
            if at.elapsed() < self.ttl {
                return Ok(key.clone());
            }
        }
        let wrapped = record.wrapped.as_ref().ok_or(SecretError::KekDestroyed)?;
        let key = self
            .kms
            .unwrap(wrapped, &kek_aad(&record.kek_id, record.version))
            .await?;
        self.cache
            .lock()
            .await
            .insert(cache_key, (Instant::now(), key.clone()));
        Ok(key)
    }
}

fn wrap_dek(
    kek: &Zeroizing<[u8; 32]>,
    aad: &[u8],
    dek: &Zeroizing<[u8; 32]>,
) -> Result<Vec<u8>, SecretError> {
    let (ciphertext, nonce) = seal_bytes(kek, aad, &dek[..])?;
    let mut blob = nonce;
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kms::LocalKms;
    use crate::legacy::MasterKey;
    use crate::store::InMemoryKekStore;

    fn kms(id: &str, fill: u8) -> LocalKms {
        LocalKms::new(id, MasterKey::from_bytes(&[fill; 32]).unwrap())
    }

    fn boxed(store: Arc<InMemoryKekStore>) -> SecretBox {
        SecretBox::new(Arc::new(kms("local:v1", 1)), store)
    }

    fn fresh() -> (SecretBox, Arc<InMemoryKekStore>) {
        let store = Arc::new(InMemoryKekStore::new());
        (boxed(store.clone()), store)
    }

    #[tokio::test]
    async fn round_trip_records_the_key_it_used() {
        let (sb, _) = fresh();
        let sealed = sb.seal("tenant:a", "a|openai", b"sk-abc").await.unwrap();
        assert_eq!(sealed.kek_id, "tenant:a");
        assert_eq!(sealed.kek_version, 1);
        assert_eq!(sealed.algorithm, ALGORITHM);
        assert_eq!(&sb.open("a|openai", &sealed).await.unwrap()[..], b"sk-abc");
    }

    #[tokio::test]
    async fn every_secret_gets_its_own_data_key() {
        let (sb, _) = fresh();
        let a = sb.seal("tenant:a", "s", b"same").await.unwrap();
        let b = sb.seal("tenant:a", "s", b"same").await.unwrap();
        assert_ne!(a.wrapped_dek, b.wrapped_dek);
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[tokio::test]
    async fn the_scope_is_kept_a_copy_into_another_scope_fails() {
        let (sb, _) = fresh();
        let sealed = sb.seal("tenant:a", "a|openai", b"sk-abc").await.unwrap();
        assert!(sb.open("b|openai", &sealed).await.is_err());
        assert!(sb.open("a|anthropic", &sealed).await.is_err());
    }

    #[tokio::test]
    async fn a_row_moved_to_another_tenant_key_fails() {
        let (sb, _) = fresh();
        let sealed = sb.seal("tenant:a", "s", b"sk-abc").await.unwrap();
        sb.seal("tenant:b", "s", b"other").await.unwrap();
        let moved = Sealed {
            kek_id: "tenant:b".into(),
            kek_version: 1,
            ..sealed
        };
        assert!(sb.open("s", &moved).await.is_err());
    }

    #[tokio::test]
    async fn changed_stored_parts_fail() {
        let (sb, _) = fresh();
        let sealed = sb.seal("tenant:a", "s", b"sk-abc").await.unwrap();
        for tamper in 0..4 {
            let mut bad = sealed.clone();
            match tamper {
                0 => bad.ciphertext[0] ^= 1,
                1 => bad.nonce[0] ^= 1,
                2 => bad.wrapped_dek[20] ^= 1,
                _ => bad.wrapped_dek.truncate(8),
            }
            assert!(sb.open("s", &bad).await.is_err(), "tamper {tamper}");
        }
        let mut other = sealed.clone();
        other.algorithm = "chacha20".into();
        assert_eq!(
            sb.open("s", &other).await.unwrap_err(),
            SecretError::Invalid("algorithm")
        );
    }

    #[tokio::test]
    async fn rotating_a_kek_keeps_old_secrets_readable_and_rewrap_moves_them() {
        let (sb, _) = fresh();
        let old = sb.seal("tenant:a", "s", b"sk-abc").await.unwrap();
        assert_eq!(sb.rotate_kek("tenant:a").await.unwrap(), 2);

        // Old secret still opens under version 1; new ones use version 2.
        assert_eq!(&sb.open("s", &old).await.unwrap()[..], b"sk-abc");
        let new = sb.seal("tenant:a", "s", b"sk-new").await.unwrap();
        assert_eq!(new.kek_version, 2);

        // Re-wrapping changes the wrapped key and version, never the ciphertext.
        let moved = sb.rewrap("s", &old).await.unwrap().expect("needs a rewrap");
        assert_eq!(moved.kek_version, 2);
        assert_eq!(moved.ciphertext, old.ciphertext);
        assert_eq!(moved.nonce, old.nonce);
        assert_ne!(moved.wrapped_dek, old.wrapped_dek);
        assert_eq!(&sb.open("s", &moved).await.unwrap()[..], b"sk-abc");

        // Nothing left to do for it, or for a secret that is already current.
        assert!(sb.rewrap("s", &moved).await.unwrap().is_none());
        assert!(sb.rewrap("s", &new).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn destroying_a_kek_makes_its_secrets_unrecoverable() {
        let (sb, store) = fresh();
        let sealed = sb.seal("tenant:a", "s", b"sk-abc").await.unwrap();
        let other = sb.seal("tenant:b", "s", b"sk-keep").await.unwrap();
        assert_eq!(sb.destroy_kek("tenant:a").await.unwrap(), 1);

        // Same box (cache cleared) and a fresh box on the same store (a restart, a backup).
        assert_eq!(
            sb.open("s", &sealed).await.unwrap_err(),
            SecretError::KekDestroyed
        );
        let restarted = boxed(store.clone());
        assert_eq!(
            restarted.open("s", &sealed).await.unwrap_err(),
            SecretError::KekDestroyed
        );
        // The wrapped key is gone from the store, not just flagged.
        assert!(store
            .get("tenant:a", 1)
            .await
            .unwrap()
            .unwrap()
            .wrapped
            .is_none());
        // Other tenants are untouched.
        assert_eq!(&sb.open("s", &other).await.unwrap()[..], b"sk-keep");
        // A destroyed id is never reused at version 1: a new seal starts after it.
        let again = sb.seal("tenant:a", "s", b"sk-new").await.unwrap();
        assert_eq!(again.kek_version, 2);
        assert!(sb.open("s", &sealed).await.is_err());
    }

    #[tokio::test]
    async fn root_key_rotation_rewraps_keks_not_secrets() {
        let store = Arc::new(InMemoryKekStore::new());
        let before = SecretBox::new(Arc::new(kms("local:v1", 1)), store.clone());
        let sealed = before.seal("tenant:a", "s", b"sk-abc").await.unwrap();

        // New root key; the old one is kept for unwrapping.
        let rotated = LocalKms::new("local:v2", MasterKey::from_bytes(&[2; 32]).unwrap())
            .with_previous("local:v1", MasterKey::from_bytes(&[1; 32]).unwrap());
        let after = SecretBox::new(Arc::new(rotated), store.clone());
        assert_eq!(&after.open("s", &sealed).await.unwrap()[..], b"sk-abc");

        assert_eq!(after.rewrap_keks(10).await.unwrap(), 1);
        assert_eq!(after.rewrap_keks(10).await.unwrap(), 0);
        let record = store.get("tenant:a", 1).await.unwrap().unwrap();
        assert_eq!(record.wrapped.unwrap().root_key_id, "local:v2");

        // With the old root key gone, the secret (unchanged) still opens.
        let only_new = SecretBox::new(
            Arc::new(LocalKms::new(
                "local:v2",
                MasterKey::from_bytes(&[2; 32]).unwrap(),
            )),
            store,
        );
        assert_eq!(&only_new.open("s", &sealed).await.unwrap()[..], b"sk-abc");
    }

    #[tokio::test]
    async fn a_wrong_root_key_cannot_open_anything() {
        let (sb, store) = fresh();
        let sealed = sb.seal("tenant:a", "s", b"sk-abc").await.unwrap();
        let wrong = SecretBox::new(Arc::new(kms("local:v1", 9)), store);
        assert!(wrong.open("s", &sealed).await.is_err());
    }

    #[tokio::test]
    async fn concurrent_first_use_creates_one_key() {
        let store = Arc::new(InMemoryKekStore::new());
        let sb = Arc::new(boxed(store.clone()));
        let mut tasks = Vec::new();
        for i in 0..8 {
            let sb = sb.clone();
            tasks.push(tokio::spawn(async move {
                sb.seal("tenant:a", "s", format!("sk-{i}").as_bytes())
                    .await
                    .unwrap()
            }));
        }
        let mut sealed = Vec::new();
        for t in tasks {
            sealed.push(t.await.unwrap());
        }
        assert!(sealed.iter().all(|s| s.kek_version == 1));
        for (i, s) in sealed.iter().enumerate() {
            assert_eq!(
                &sb.open("s", s).await.unwrap()[..],
                format!("sk-{i}").as_bytes()
            );
        }
        assert_eq!(store.latest_version("tenant:a").await.unwrap(), Some(1));
    }

    #[tokio::test]
    async fn debug_output_shows_no_key_material() {
        let (sb, _) = fresh();
        let sealed = sb.seal("tenant:a", "s", b"sk-abc").await.unwrap();
        let shown = format!("{sealed:?} {:?}", kms("local:v1", 1));
        assert!(!shown.contains("sk-abc"));
        assert!(shown.contains("tenant:a"));
        assert!(!shown.contains("ciphertext:"));
    }
}
