//! Maintenance of sealed tenant secrets (ADR-014, MOH-33): moving earlier secrets to envelope
//! encryption, re-wrapping data keys after a KEK rotation, and rotating keys.
//!
//! Run through `server secrets <action>`; see the runbook (`docs/runbooks/operations.md`).
//!
//! - [`run`] with [`Op::Reencrypt`] opens every secret that is still sealed the earlier way (one
//!   static key), seals it under its own data key, **checks the new seal opens to the same
//!   plaintext**, and stores it only while the row still holds the ciphertext that was read. A
//!   secret that is changed meanwhile is left as the user set it. Readers open both kinds of row,
//!   so there is no downtime and the job can be stopped and run again.
//! - [`run`] with [`Op::Rewrap`] wraps each data key again under its tenant's active KEK, without
//!   touching the secret. It is the second half of a KEK rotation.
//!
//! The job works one tenant at a time through that tenant's Unit of Work, so row-level security
//! applies as it does for the application; platform credentials go through the audited
//! `xtenant` functions. Plaintext never leaves the `Vault` and nothing here logs it.

use config::AppConfig;
use secrets::{StoredSecret, Vault};
use sqlx::PgPool;
use uuid::Uuid;

/// What a run does to each secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Seal earlier secrets under envelope encryption.
    Reencrypt,
    /// Wrap data keys under the active KEK of their tenant.
    Rewrap,
}

/// The result of a run. The work is complete when `failed` is 0 and a second run changes nothing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Secrets looked at.
    pub scanned: u64,
    /// Secrets rewritten.
    pub changed: u64,
    /// Secrets that could not be moved (did not open, did not verify, or the store failed).
    pub failed: u64,
    /// Secrets that changed while the job ran and were left as they are now.
    pub raced: u64,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    AiCredential,
    IntegrationSecret,
    WebhookToken,
    ToolCredential,
}

impl Kind {
    const ALL: [Kind; 4] = [
        Kind::AiCredential,
        Kind::IntegrationSecret,
        Kind::WebhookToken,
        Kind::ToolCredential,
    ];

    fn label(self) -> &'static str {
        match self {
            Kind::AiCredential => "ai credential",
            Kind::IntegrationSecret => "integration secret",
            Kind::WebhookToken => "webhook token",
            Kind::ToolCredential => "tool credential",
        }
    }
}

/// `id, <name>, ciphertext, nonce` and the four envelope columns.
type SealedRow = (
    Uuid,
    String,
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
);

/// An integration secret: `id, slug, field_key`, then the same columns.
type IntegrationSecretRow = (
    Uuid,
    String,
    String,
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
);

/// A tool credential: `id, name, credential text` and the four envelope columns.
type ToolRow = (
    Uuid,
    String,
    String,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
);

struct Row {
    id: Uuid,
    aad: String,
    stored: StoredSecret,
}

type Columns = (
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
);

fn stored(columns: Columns) -> Result<StoredSecret, String> {
    let (ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm) = columns;
    StoredSecret::from_columns(
        ciphertext,
        nonce,
        wrapped_dek,
        kek_id,
        kek_version,
        algorithm,
    )
    .map_err(|e| e.to_string())
}

/// Seal or re-wrap every sealed secret: the platform's, then each live tenant's.
pub async fn run(pool: &PgPool, config: &AppConfig, op: Op) -> Result<Report, String> {
    let ai_vault = Vault::from_config(
        pool,
        config.ai_key_encryption_key.as_deref(),
        config.secrets_root_key(),
        config.secrets_root_key_previous.as_deref(),
    )?;
    let integration_vault = Vault::from_config(
        pool,
        config.integration_secrets_key.as_deref(),
        config.secrets_root_key(),
        config.secrets_root_key_previous.as_deref(),
    )?;
    if !ai_vault.can_seal() {
        return Err(
            "no root key is configured (APP_SECRETS_ROOT_KEY, or the AI or integration key)".into(),
        );
    }

    let mut report = Report::default();

    for provider in ["openai", "anthropic", "gemini"] {
        platform_credential(pool, &ai_vault, provider, op, &mut report).await;
    }

    let tenants: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM tenants WHERE deleted_at IS NULL ORDER BY id")
            .fetch_all(pool)
            .await
            .map_err(|e| format!("could not list tenants: {e}"))?;

    for tenant_id in tenants {
        for kind in Kind::ALL {
            let vault = match kind {
                Kind::IntegrationSecret | Kind::WebhookToken => &integration_vault,
                Kind::AiCredential | Kind::ToolCredential => &ai_vault,
            };
            let rows = match load(pool, tenant_id, kind).await {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!(%tenant_id, kind = kind.label(), error = %e, "could not read secrets");
                    report.failed += 1;
                    continue;
                }
            };
            for row in rows {
                report.scanned += 1;
                let moved = match op {
                    Op::Reencrypt => {
                        vault
                            .reencrypt(Some(tenant_id), &row.aad, &row.stored)
                            .await
                    }
                    Op::Rewrap => vault.rewrap(&row.aad, &row.stored).await,
                };
                match moved {
                    Ok(None) => {}
                    Ok(Some(new)) => match save(pool, tenant_id, kind, &row, &new).await {
                        Ok(true) => report.changed += 1,
                        Ok(false) => report.raced += 1,
                        Err(e) => {
                            tracing::error!(%tenant_id, kind = kind.label(), id = %row.id, error = %e, "could not store the sealed secret");
                            report.failed += 1;
                        }
                    },
                    Err(e) => {
                        // The message names no key or plaintext.
                        tracing::error!(%tenant_id, kind = kind.label(), id = %row.id, error = %e, "secret not moved");
                        report.failed += 1;
                    }
                }
            }
        }
    }
    Ok(report)
}

async fn load(pool: &PgPool, tenant_id: Uuid, kind: Kind) -> Result<Vec<Row>, String> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id)
        .await
        .map_err(|e| e.to_string())?;
    let rows = match kind {
        Kind::AiCredential => {
            let raw: Vec<SealedRow> =
                sqlx::query_as(
                    "SELECT id, provider, ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm \
                     FROM ai_credentials WHERE tenant_id = $1 ORDER BY id",
                )
                .bind(tenant_id)
                .fetch_all(uow.conn())
                .await
                .map_err(|e| e.to_string())?;
            raw.into_iter()
                .map(|(id, provider, ct, nonce, dek, kek, ver, alg)| {
                    Ok(Row {
                        id,
                        aad: ai::crypto::aad(Some(tenant_id), &provider),
                        stored: stored((ct, nonce, dek, kek, ver, alg))?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        Kind::IntegrationSecret => {
            let raw: Vec<IntegrationSecretRow> = sqlx::query_as(
                "SELECT s.id, cat.slug, s.field_key, s.ciphertext, s.nonce, s.wrapped_dek, \
                            s.kek_id, s.kek_version, s.algorithm \
                     FROM integration_secrets s \
                     JOIN integration_connections c ON c.id = s.connection_id \
                     JOIN integration_catalog cat ON cat.id = c.catalog_id \
                     WHERE s.tenant_id = $1 ORDER BY s.id",
            )
            .bind(tenant_id)
            .fetch_all(uow.conn())
            .await
            .map_err(|e| e.to_string())?;
            raw.into_iter()
                .map(|(id, slug, field, ct, nonce, dek, kek, ver, alg)| {
                    Ok(Row {
                        id,
                        aad: integrations::crypto::aad(tenant_id, &slug, &field),
                        stored: stored((ct, nonce, dek, kek, ver, alg))?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        Kind::WebhookToken => {
            let raw: Vec<SealedRow> = sqlx::query_as(
                "SELECT c.id, cat.slug, c.webhook_token_ciphertext, c.webhook_token_nonce, \
                            c.webhook_token_wrapped_dek, c.webhook_token_kek_id, \
                            c.webhook_token_kek_version, c.webhook_token_algorithm \
                     FROM integration_connections c \
                     JOIN integration_catalog cat ON cat.id = c.catalog_id \
                     WHERE c.tenant_id = $1 ORDER BY c.id",
            )
            .bind(tenant_id)
            .fetch_all(uow.conn())
            .await
            .map_err(|e| e.to_string())?;
            raw.into_iter()
                .map(|(id, slug, ct, nonce, dek, kek, ver, alg)| {
                    Ok(Row {
                        id,
                        aad: integrations::crypto::aad(
                            tenant_id,
                            &slug,
                            integrations::routes::FIELD_WEBHOOK_TOKEN,
                        ),
                        stored: stored((ct, nonce, dek, kek, ver, alg))?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        Kind::ToolCredential => {
            let raw: Vec<ToolRow> = sqlx::query_as(
                "SELECT id, name, credential_ciphertext, credential_wrapped_dek, \
                            credential_kek_id, credential_kek_version, credential_algorithm \
                     FROM tenant_tools \
                     WHERE tenant_id = $1 AND credential_ciphertext IS NOT NULL ORDER BY id",
            )
            .bind(tenant_id)
            .fetch_all(uow.conn())
            .await
            .map_err(|e| e.to_string())?;
            raw.into_iter()
                .map(|(id, name, text, dek, kek, ver, alg)| {
                    Ok(Row {
                        id,
                        aad: ai::crypto::aad(Some(tenant_id), &name),
                        stored: tools::credential::decode(&text, dek, kek, ver, alg)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
    };
    drop(uow);
    Ok(rows)
}

/// Store `new` for `row`, only while the row still holds what was read. `Ok(false)` means it
/// changed in between. Tool credentials keep `base64(ciphertext)||base64(nonce)` as text in one
/// column; the other kinds have a ciphertext and a nonce column.
async fn save(
    pool: &PgPool,
    tenant_id: Uuid,
    kind: Kind,
    row: &Row,
    new: &StoredSecret,
) -> Result<bool, String> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id)
        .await
        .map_err(|e| e.to_string())?;
    let old = &row.stored;
    let result =
        match kind {
            Kind::ToolCredential => sqlx::query(
                "UPDATE tenant_tools SET credential_ciphertext = $3, credential_wrapped_dek = $4, \
                        credential_kek_id = $5, credential_kek_version = $6, \
                        credential_algorithm = $7 \
                 WHERE id = $1 AND tenant_id = $2 AND credential_ciphertext = $8 \
                   AND credential_wrapped_dek IS NOT DISTINCT FROM $9",
            )
            .bind(row.id)
            .bind(tenant_id)
            .bind(tools::credential::encode(new))
            .bind(new.wrapped_dek())
            .bind(new.kek_id())
            .bind(new.kek_version())
            .bind(new.algorithm())
            .bind(tools::credential::encode(old))
            .bind(old.wrapped_dek())
            .execute(uow.conn())
            .await,
            _ => {
                let sql = match kind {
                Kind::AiCredential => {
                    "UPDATE ai_credentials SET ciphertext = $3, nonce = $4, wrapped_dek = $5, \
                            kek_id = $6, kek_version = $7, algorithm = $8 \
                     WHERE id = $1 AND tenant_id = $2 AND ciphertext = $9 AND nonce = $10 \
                       AND wrapped_dek IS NOT DISTINCT FROM $11"
                }
                Kind::IntegrationSecret => {
                    "UPDATE integration_secrets SET ciphertext = $3, nonce = $4, wrapped_dek = $5, \
                            kek_id = $6, kek_version = $7, algorithm = $8 \
                     WHERE id = $1 AND tenant_id = $2 AND ciphertext = $9 AND nonce = $10 \
                       AND wrapped_dek IS NOT DISTINCT FROM $11"
                }
                _ => {
                    "UPDATE integration_connections SET webhook_token_ciphertext = $3, \
                            webhook_token_nonce = $4, webhook_token_wrapped_dek = $5, \
                            webhook_token_kek_id = $6, webhook_token_kek_version = $7, \
                            webhook_token_algorithm = $8 \
                     WHERE id = $1 AND tenant_id = $2 AND webhook_token_ciphertext = $9 \
                       AND webhook_token_nonce = $10 \
                       AND webhook_token_wrapped_dek IS NOT DISTINCT FROM $11"
                }
            };
                sqlx::query(sql)
                    .bind(row.id)
                    .bind(tenant_id)
                    .bind(&new.ciphertext)
                    .bind(&new.nonce)
                    .bind(new.wrapped_dek())
                    .bind(new.kek_id())
                    .bind(new.kek_version())
                    .bind(new.algorithm())
                    .bind(&old.ciphertext)
                    .bind(&old.nonce)
                    .bind(old.wrapped_dek())
                    .execute(uow.conn())
                    .await
            }
        }
        .map_err(|e| e.to_string())?;
    let changed = result.rows_affected() == 1;
    uow.commit().await.map_err(|e| e.to_string())?;
    Ok(changed)
}

async fn platform_credential(
    pool: &PgPool,
    vault: &Vault,
    provider: &str,
    op: Op,
    report: &mut Report,
) {
    let row: Result<Option<Columns>, sqlx::Error> = sqlx::query_as(
        "SELECT ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm \
         FROM xtenant.platform_ai_credential_secret($1)",
    )
    .bind(provider)
    .fetch_optional(pool)
    .await;
    let current = match row
        .map_err(|e| e.to_string())
        .and_then(|r| r.map(stored).transpose())
    {
        Ok(Some(current)) => current,
        Ok(None) => return,
        Err(e) => {
            tracing::error!(provider, error = %e, "could not read the platform credential");
            report.failed += 1;
            return;
        }
    };
    report.scanned += 1;
    let aad = ai::crypto::aad(None, provider);
    let moved = match op {
        Op::Reencrypt => vault.reencrypt(None, &aad, &current).await,
        Op::Rewrap => vault.rewrap(&aad, &current).await,
    };
    match moved {
        Ok(None) => {}
        Ok(Some(new)) => {
            let saved: Result<bool, sqlx::Error> = sqlx::query_scalar(
                "SELECT xtenant.platform_ai_credential_reseal($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(provider)
            .bind(&current.ciphertext)
            .bind(&new.ciphertext)
            .bind(&new.nonce)
            .bind(new.wrapped_dek())
            .bind(new.kek_id())
            .bind(new.kek_version())
            .bind(new.algorithm())
            .fetch_one(pool)
            .await;
            match saved {
                Ok(true) => report.changed += 1,
                Ok(false) => report.raced += 1,
                Err(e) => {
                    tracing::error!(provider, error = %e, "could not store the platform credential");
                    report.failed += 1;
                }
            }
        }
        Err(e) => {
            tracing::error!(provider, error = %e, "platform credential not moved");
            report.failed += 1;
        }
    }
}

/// Start a new KEK version for a tenant (`Some`) or the platform (`None`). Secrets keep opening
/// under the old version until [`run`] with [`Op::Rewrap`] has moved them.
pub async fn rotate_kek(
    pool: &PgPool,
    config: &AppConfig,
    tenant_id: Option<Uuid>,
) -> Result<i32, String> {
    let vault = Vault::from_config(
        pool,
        None,
        config.secrets_root_key(),
        config.secrets_root_key_previous.as_deref(),
    )?;
    let secret_box = vault
        .secret_box()
        .ok_or("no root key is configured (APP_SECRETS_ROOT_KEY, or the AI or integration key)")?;
    secret_box
        .rotate_kek(&secrets::kek_id(tenant_id))
        .await
        .map_err(|e| e.to_string())
}

/// Wrap every KEK that an earlier root key still wraps under the current root key, until none is
/// left. Returns how many were moved.
pub async fn rewrap_keks(pool: &PgPool, config: &AppConfig) -> Result<u64, String> {
    let vault = Vault::from_config(
        pool,
        None,
        config.secrets_root_key(),
        config.secrets_root_key_previous.as_deref(),
    )?;
    let secret_box = vault
        .secret_box()
        .ok_or("no root key is configured (APP_SECRETS_ROOT_KEY, or the AI or integration key)")?;
    let mut total = 0;
    loop {
        let moved = secret_box
            .rewrap_keks(100)
            .await
            .map_err(|e| e.to_string())?;
        if moved == 0 {
            return Ok(total);
        }
        total += moved;
    }
}
