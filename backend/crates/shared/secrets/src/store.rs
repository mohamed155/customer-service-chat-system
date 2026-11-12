//! Where wrapped key-encryption keys (KEKs) are kept: one row per `(kek_id, version)`.
//!
//! A KEK id is `tenant:<uuid>` or `platform`. At most one version is active; rotation retires the
//! old one and activates a new one in a single step. Destroying a KEK clears its wrapped key, which
//! is what makes the secrets under it unrecoverable (crypto-shredding, ADR-014 section 4).
//! The table is platform-level, not tenant data: it holds only wrapped keys and has no tenant
//! column (it is listed in `GLOBAL_TABLES`).

use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use sqlx::PgPool;

use crate::kms::WrappedKey;
use crate::SecretError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KekState {
    Active,
    Retired,
    Destroyed,
}

impl KekState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
            Self::Destroyed => "destroyed",
        }
    }

    fn parse(value: &str) -> Result<Self, SecretError> {
        match value {
            "active" => Ok(Self::Active),
            "retired" => Ok(Self::Retired),
            "destroyed" => Ok(Self::Destroyed),
            _ => Err(SecretError::Invalid("key state")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KekRecord {
    pub kek_id: String,
    pub version: i32,
    pub state: KekState,
    /// `None` once destroyed.
    pub wrapped: Option<WrappedKey>,
}

#[async_trait]
pub trait KekStore: Send + Sync {
    async fn active(&self, kek_id: &str) -> Result<Option<KekRecord>, SecretError>;
    async fn get(&self, kek_id: &str, version: i32) -> Result<Option<KekRecord>, SecretError>;
    /// The highest version of a KEK, destroyed ones included (a destroyed KEK is never reused).
    async fn latest_version(&self, kek_id: &str) -> Result<Option<i32>, SecretError>;
    /// Insert an active KEK. `false` when that version already exists or another version is
    /// active (a concurrent first use); the caller reloads the active one.
    async fn insert_active(&self, record: &KekRecord) -> Result<bool, SecretError>;
    /// Retire the active version and activate `next`, together. `false` if the active version
    /// changed meanwhile.
    async fn rotate(
        &self,
        kek_id: &str,
        retire_version: i32,
        next: &KekRecord,
    ) -> Result<bool, SecretError>;
    /// Replace a KEK's wrapped key (root-key rotation), only if it is still wrapped by
    /// `expected_root`.
    async fn replace_wrapped(
        &self,
        kek_id: &str,
        version: i32,
        expected_root: &str,
        wrapped: &WrappedKey,
    ) -> Result<bool, SecretError>;
    /// KEKs not yet wrapped by `root_key_id`, destroyed ones excluded.
    async fn not_wrapped_by(
        &self,
        root_key_id: &str,
        limit: i64,
    ) -> Result<Vec<KekRecord>, SecretError>;
    /// Destroy every version of a KEK. Returns how many versions were destroyed.
    async fn destroy(&self, kek_id: &str) -> Result<u64, SecretError>;
}

/// PostgreSQL store (`secret_keks`).
#[derive(Clone)]
pub struct PgKekStore {
    pool: PgPool,
}

impl PgKekStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

type Row = (String, i32, String, Option<String>, Option<Vec<u8>>);

fn record(row: Row) -> Result<KekRecord, SecretError> {
    let (kek_id, version, state, root, blob) = row;
    let wrapped = match (root, blob) {
        (Some(root_key_id), Some(blob)) => Some(WrappedKey { root_key_id, blob }),
        _ => None,
    };
    Ok(KekRecord {
        kek_id,
        version,
        state: KekState::parse(&state)?,
        wrapped,
    })
}

#[async_trait]
impl KekStore for PgKekStore {
    async fn active(&self, kek_id: &str) -> Result<Option<KekRecord>, SecretError> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT kek_id, version, state, root_key_id, wrapped_kek \
             FROM secret_keks WHERE kek_id = $1 AND state = 'active'",
        )
        .bind(kek_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(record).transpose()
    }

    async fn get(&self, kek_id: &str, version: i32) -> Result<Option<KekRecord>, SecretError> {
        let row: Option<Row> = sqlx::query_as(
            "SELECT kek_id, version, state, root_key_id, wrapped_kek \
             FROM secret_keks WHERE kek_id = $1 AND version = $2",
        )
        .bind(kek_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;
        row.map(record).transpose()
    }

    async fn latest_version(&self, kek_id: &str) -> Result<Option<i32>, SecretError> {
        let version: Option<i32> =
            sqlx::query_scalar("SELECT max(version) FROM secret_keks WHERE kek_id = $1")
                .bind(kek_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(version)
    }

    async fn insert_active(&self, record: &KekRecord) -> Result<bool, SecretError> {
        let wrapped = record
            .wrapped
            .as_ref()
            .ok_or(SecretError::Invalid("an active key needs a wrapped key"))?;
        let result = sqlx::query(
            "INSERT INTO secret_keks (kek_id, version, state, root_key_id, wrapped_kek) \
             VALUES ($1, $2, 'active', $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(&record.kek_id)
        .bind(record.version)
        .bind(&wrapped.root_key_id)
        .bind(&wrapped.blob)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn rotate(
        &self,
        kek_id: &str,
        retire_version: i32,
        next: &KekRecord,
    ) -> Result<bool, SecretError> {
        let wrapped = next
            .wrapped
            .as_ref()
            .ok_or(SecretError::Invalid("an active key needs a wrapped key"))?;
        let mut tx = self.pool.begin().await?;
        let retired = sqlx::query(
            "UPDATE secret_keks SET state = 'retired', retired_at = now() \
             WHERE kek_id = $1 AND version = $2 AND state = 'active'",
        )
        .bind(kek_id)
        .bind(retire_version)
        .execute(&mut *tx)
        .await?;
        if retired.rows_affected() != 1 {
            return Ok(false);
        }
        let inserted = sqlx::query(
            "INSERT INTO secret_keks (kek_id, version, state, root_key_id, wrapped_kek) \
             VALUES ($1, $2, 'active', $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(kek_id)
        .bind(next.version)
        .bind(&wrapped.root_key_id)
        .bind(&wrapped.blob)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() != 1 {
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    async fn replace_wrapped(
        &self,
        kek_id: &str,
        version: i32,
        expected_root: &str,
        wrapped: &WrappedKey,
    ) -> Result<bool, SecretError> {
        let result = sqlx::query(
            "UPDATE secret_keks SET root_key_id = $4, wrapped_kek = $5 \
             WHERE kek_id = $1 AND version = $2 AND root_key_id = $3 AND state <> 'destroyed'",
        )
        .bind(kek_id)
        .bind(version)
        .bind(expected_root)
        .bind(&wrapped.root_key_id)
        .bind(&wrapped.blob)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn not_wrapped_by(
        &self,
        root_key_id: &str,
        limit: i64,
    ) -> Result<Vec<KekRecord>, SecretError> {
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT kek_id, version, state, root_key_id, wrapped_kek FROM secret_keks \
             WHERE state <> 'destroyed' AND root_key_id <> $1 ORDER BY kek_id, version LIMIT $2",
        )
        .bind(root_key_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(record).collect()
    }

    async fn destroy(&self, kek_id: &str) -> Result<u64, SecretError> {
        let result = sqlx::query(
            "UPDATE secret_keks SET state = 'destroyed', wrapped_kek = NULL, root_key_id = NULL, \
             destroyed_at = now() WHERE kek_id = $1 AND state <> 'destroyed'",
        )
        .bind(kek_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

/// A store in memory, for tests of code that uses [`SecretBox`](crate::SecretBox) and for the
/// unit tests of this crate.
#[derive(Default)]
pub struct InMemoryKekStore {
    rows: Mutex<BTreeMap<(String, i32), KekRecord>>,
}

impl InMemoryKekStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, i32), KekRecord>> {
        self.rows.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl KekStore for InMemoryKekStore {
    async fn active(&self, kek_id: &str) -> Result<Option<KekRecord>, SecretError> {
        Ok(self
            .lock()
            .values()
            .find(|r| r.kek_id == kek_id && r.state == KekState::Active)
            .cloned())
    }

    async fn get(&self, kek_id: &str, version: i32) -> Result<Option<KekRecord>, SecretError> {
        Ok(self.lock().get(&(kek_id.to_owned(), version)).cloned())
    }

    async fn latest_version(&self, kek_id: &str) -> Result<Option<i32>, SecretError> {
        Ok(self
            .lock()
            .values()
            .filter(|r| r.kek_id == kek_id)
            .map(|r| r.version)
            .max())
    }

    async fn insert_active(&self, record: &KekRecord) -> Result<bool, SecretError> {
        let mut rows = self.lock();
        let taken = rows.values().any(|r| {
            r.kek_id == record.kek_id
                && (r.version == record.version || r.state == KekState::Active)
        });
        if taken {
            return Ok(false);
        }
        rows.insert((record.kek_id.clone(), record.version), record.clone());
        Ok(true)
    }

    async fn rotate(
        &self,
        kek_id: &str,
        retire_version: i32,
        next: &KekRecord,
    ) -> Result<bool, SecretError> {
        let mut rows = self.lock();
        let key = (kek_id.to_owned(), retire_version);
        match rows.get(&key) {
            Some(r) if r.state == KekState::Active => {}
            _ => return Ok(false),
        }
        if rows.contains_key(&(kek_id.to_owned(), next.version)) {
            return Ok(false);
        }
        if let Some(r) = rows.get_mut(&key) {
            r.state = KekState::Retired;
        }
        rows.insert((kek_id.to_owned(), next.version), next.clone());
        Ok(true)
    }

    async fn replace_wrapped(
        &self,
        kek_id: &str,
        version: i32,
        expected_root: &str,
        wrapped: &WrappedKey,
    ) -> Result<bool, SecretError> {
        let mut rows = self.lock();
        match rows.get_mut(&(kek_id.to_owned(), version)) {
            Some(r)
                if r.state != KekState::Destroyed
                    && r.wrapped.as_ref().map(|w| w.root_key_id.as_str())
                        == Some(expected_root) =>
            {
                r.wrapped = Some(wrapped.clone());
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn not_wrapped_by(
        &self,
        root_key_id: &str,
        limit: i64,
    ) -> Result<Vec<KekRecord>, SecretError> {
        Ok(self
            .lock()
            .values()
            .filter(|r| {
                r.state != KekState::Destroyed
                    && r.wrapped.as_ref().map(|w| w.root_key_id.as_str()) != Some(root_key_id)
            })
            .take(limit.max(0) as usize)
            .cloned()
            .collect())
    }

    async fn destroy(&self, kek_id: &str) -> Result<u64, SecretError> {
        let mut count = 0;
        for r in self.lock().values_mut() {
            if r.kek_id == kek_id && r.state != KekState::Destroyed {
                r.state = KekState::Destroyed;
                r.wrapped = None;
                count += 1;
            }
        }
        Ok(count)
    }
}
