use ai_providers::SecretKey;
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

pub async fn resolve_config(
    pool: &PgPool,
    scope: Scope,
) -> Result<Option<ResolvedConfig>, sqlx::Error> {
    match scope {
        Scope::Tenant(tenant_id) => {
            let row = sqlx::query_as::<_, AiConfigRow>(
                "SELECT * FROM ai_configurations \
                 WHERE (tenant_id = $1 OR tenant_id IS NULL) AND deleted_at IS NULL \
                 ORDER BY tenant_id NULLS LAST LIMIT 1",
            )
            .bind(tenant_id)
            .fetch_optional(pool)
            .await?;

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
            let row = sqlx::query_as::<_, AiConfigRow>(
                "SELECT * FROM ai_configurations \
                 WHERE tenant_id IS NULL AND deleted_at IS NULL \
                 LIMIT 1",
            )
            .fetch_optional(pool)
            .await?;

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
    master: &crypto::MasterKey,
    scope: Scope,
    provider: &str,
) -> Result<Option<(SecretKey, bool)>, String> {
    match scope {
        Scope::Tenant(tenant_id) => {
            let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>, Option<Uuid>)>(
                "SELECT ciphertext, nonce, tenant_id FROM ai_credentials \
                 WHERE (tenant_id = $1 OR tenant_id IS NULL) AND provider = $2 AND deleted_at IS NULL \
                 ORDER BY tenant_id NULLS LAST LIMIT 1",
            )
            .bind(tenant_id)
            .bind(provider)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("credential query: {e}"))?;

            if let Some((ciphertext, nonce, tid)) = row {
                let aad = crypto::aad(tid, provider);
                let key = crypto::open(master, &aad, &ciphertext, &nonce)?;
                return Ok(Some((key, tid.is_some())));
            }

            Ok(None)
        }
        Scope::Platform => {
            let row = sqlx::query_as::<_, (Vec<u8>, Vec<u8>)>(
                "SELECT ciphertext, nonce FROM ai_credentials \
                 WHERE tenant_id IS NULL AND provider = $1 AND deleted_at IS NULL \
                 LIMIT 1",
            )
            .bind(provider)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("credential query: {e}"))?;

            if let Some((ciphertext, nonce)) = row {
                let aad = crypto::aad(None, provider);
                let key = crypto::open(master, &aad, &ciphertext, &nonce)?;
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
            let row = sqlx::query_as::<_, (String, Option<Uuid>)>(
                "SELECT key_hint, tenant_id FROM ai_credentials \
                 WHERE (tenant_id = $1 OR tenant_id IS NULL) AND provider = $2 AND deleted_at IS NULL \
                 ORDER BY tenant_id NULLS LAST LIMIT 1",
            )
            .bind(tenant_id)
            .bind(provider)
            .fetch_optional(pool)
            .await
            .ok()?;

            row.map(|(hint, tid)| CredentialView {
                source: if tid.is_some() {
                    "tenant".into()
                } else {
                    "platform".into()
                },
                provider: provider.to_string(),
                key_hint: hint,
            })
        }
        Scope::Platform => {
            let row = sqlx::query_as::<_, (String,)>(
                "SELECT key_hint FROM ai_credentials \
                 WHERE tenant_id IS NULL AND provider = $1 AND deleted_at IS NULL \
                 LIMIT 1",
            )
            .bind(provider)
            .fetch_optional(pool)
            .await
            .ok()?;

            row.map(|(hint,)| CredentialView {
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
