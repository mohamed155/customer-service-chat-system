use ai_providers::SecretKey;
use secrets::{StoredSecret, Vault};
use sqlx::PgPool;
use uuid::Uuid;

use crate::crypto;
use crate::model::{AiConfigRow, CredentialView};

#[derive(Clone, Copy)]
pub enum Scope {
    Tenant(Uuid),
    Platform,
}

pub struct ResolvedConfig {
    pub row: AiConfigRow,
    pub scope_is_tenant: bool,
    pub capture_content: bool,
}

// The platform defaults (rows with no tenant) are hidden from tenant queries by the row-level
// policy on `ai_configurations` and `ai_credentials`, on purpose: a bug in tenant-scoped code must
// not be able to read the platform's API keys. A tenant resolves its own row in its Unit of Work
// and falls back to the platform default only through the audited functions below (MOH-48).

/// The platform default configuration, through `xtenant.platform_ai_config`.
async fn platform_config(pool: &PgPool) -> Result<Option<AiConfigRow>, sqlx::Error> {
    sqlx::query_as::<_, AiConfigRow>("SELECT * FROM xtenant.platform_ai_config()")
        .fetch_optional(pool)
        .await
}

type SecretColumns = (
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
);

/// The same columns plus the row's `tenant_id` (a flat tuple: sqlx cannot decode a nested one).
type TenantSecretColumns = (
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
    Option<Uuid>,
);

fn stored(columns: SecretColumns) -> Result<StoredSecret, String> {
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

/// Open a stored credential into a provider key.
async fn open_key(vault: &Vault, aad: &str, secret: &StoredSecret) -> Result<SecretKey, String> {
    let plaintext = vault.open(aad, secret).await.map_err(|e| e.to_string())?;
    crypto::secret_key(&plaintext)
}

/// The platform's sealed key for a provider, through `xtenant.platform_ai_credential_secret`.
async fn platform_secret(pool: &PgPool, provider: &str) -> Result<Option<StoredSecret>, String> {
    sqlx::query_as::<_, SecretColumns>(
        "SELECT ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm \
         FROM xtenant.platform_ai_credential_secret($1)",
    )
    .bind(provider)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("credential query: {e}"))?
    .map(stored)
    .transpose()
}

/// The platform key's display hint for a provider, through `xtenant.platform_ai_credential_hint`.
async fn platform_hint(pool: &PgPool, provider: &str) -> Result<Option<String>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (String,)>(
        "SELECT key_hint FROM xtenant.platform_ai_credential_hint($1)",
    )
    .bind(provider)
    .fetch_optional(pool)
    .await?
    .map(|(hint,)| hint))
}

pub async fn resolve_config(
    pool: &PgPool,
    scope: Scope,
) -> Result<Option<ResolvedConfig>, sqlx::Error> {
    match scope {
        Scope::Tenant(tenant_id) => {
            let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
            let own = sqlx::query_as::<_, AiConfigRow>(
                "SELECT * FROM ai_configurations \
                 WHERE tenant_id = $1 AND deleted_at IS NULL \
                 LIMIT 1",
            )
            .bind(tenant_id)
            .fetch_optional(uow.conn())
            .await?;
            drop(uow);

            let row = match own {
                Some(row) => Some(row),
                None => platform_config(pool).await?,
            };

            Ok(row.map(|row| {
                let is_tenant = row.tenant_id.is_some();
                ResolvedConfig {
                    capture_content: if is_tenant {
                        row.capture_content
                    } else {
                        false
                    },
                    scope_is_tenant: is_tenant,
                    row,
                }
            }))
        }
        Scope::Platform => {
            let row = platform_config(pool).await?;

            Ok(row.map(|row| ResolvedConfig {
                capture_content: false,
                scope_is_tenant: false,
                row,
            }))
        }
    }
}

pub async fn resolve_credential(
    pool: &PgPool,
    vault: &Vault,
    scope: Scope,
    provider: &str,
) -> Result<Option<(SecretKey, bool)>, String> {
    match scope {
        Scope::Tenant(tenant_id) => {
            let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id)
                .await
                .map_err(|e| format!("credential query: {e}"))?;
            let row = sqlx::query_as::<_, TenantSecretColumns>(
                "SELECT ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm, tenant_id \
                 FROM ai_credentials \
                 WHERE tenant_id = $1 AND provider = $2 AND deleted_at IS NULL \
                 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(provider)
            .fetch_optional(uow.conn())
            .await
            .map_err(|e| format!("credential query: {e}"))?;
            drop(uow);

            if let Some((ciphertext, nonce, dek, kek_id, kek_version, algorithm, tid)) = row {
                let secret = stored((ciphertext, nonce, dek, kek_id, kek_version, algorithm))?;
                let aad = crypto::aad(tid, provider);
                let key = open_key(vault, &aad, &secret).await?;
                return Ok(Some((key, tid.is_some())));
            }

            // No key of its own: the platform's.
            if let Some(secret) = platform_secret(pool, provider).await? {
                let aad = crypto::aad(None, provider);
                let key = open_key(vault, &aad, &secret).await?;
                return Ok(Some((key, false)));
            }

            Ok(None)
        }
        Scope::Platform => {
            if let Some(secret) = platform_secret(pool, provider).await? {
                let aad = crypto::aad(None, provider);
                let key = open_key(vault, &aad, &secret).await?;
                return Ok(Some((key, false)));
            }

            Ok(None)
        }
    }
}

pub async fn resolve_credential_view(
    pool: &PgPool,
    scope: Scope,
    provider: &str,
) -> Option<CredentialView> {
    match scope {
        Scope::Tenant(tenant_id) => {
            let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id)
                .await
                .ok()?;
            let row = sqlx::query_as::<_, (String, Option<Uuid>)>(
                "SELECT key_hint, tenant_id FROM ai_credentials \
                 WHERE tenant_id = $1 AND provider = $2 AND deleted_at IS NULL \
                 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(provider)
            .fetch_optional(uow.conn())
            .await
            .ok()?;
            drop(uow);

            if let Some((hint, tid)) = row {
                return Some(CredentialView {
                    source: if tid.is_some() {
                        "tenant".into()
                    } else {
                        "platform".into()
                    },
                    provider: provider.to_string(),
                    key_hint: hint,
                });
            }

            // No key of its own: show the platform's hint.
            platform_hint(pool, provider)
                .await
                .ok()?
                .map(|hint| CredentialView {
                    source: "platform".into(),
                    provider: provider.to_string(),
                    key_hint: hint,
                })
        }
        Scope::Platform => {
            let hint = platform_hint(pool, provider).await.ok()?;

            hint.map(|hint| CredentialView {
                source: "platform".into(),
                provider: provider.to_string(),
                key_hint: hint,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::aad;
    use uuid::Uuid;

    #[test]
    fn test_aad_tenant_format() {
        let tenant_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        assert_eq!(
            aad(Some(tenant_id), "openai"),
            "11111111-1111-4111-8111-111111111111|openai"
        );
    }

    #[test]
    fn test_aad_platform_format() {
        assert_eq!(aad(None, "anthropic"), "platform|anthropic");
    }
}
