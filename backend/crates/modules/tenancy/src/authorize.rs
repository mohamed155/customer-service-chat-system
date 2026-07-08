//! Authorization queries — tenant existence and membership checks.

use sqlx::PgPool;
use uuid::Uuid;

/// A row from the `tenants` table (non-deleted only).
#[derive(Debug, Clone)]
pub struct TenantRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
}

/// Look up a tenant by its primary key, returning `None` when the tenant
/// does not exist or has been soft-deleted (`deleted_at IS NOT NULL`).
pub async fn fetch_tenant(pool: &PgPool, id: Uuid) -> Option<TenantRow> {
    let result = sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT id, name, slug, status \
         FROM tenants \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some((id, name, slug, status))) => Some(TenantRow {
            id,
            name,
            slug,
            status,
        }),
        _ => None,
    }
}

/// Returns `true` when `user_id` has an active (non-deleted) membership in
/// the tenant identified by `tenant_id`.
pub async fn has_active_membership(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> bool {
    let result = sqlx::query_scalar::<_, i32>(
        "SELECT 1 \
         FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await;

    matches!(result, Ok(Some(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_row_size() {
        assert_eq!(
            size_of::<TenantRow>(),
            size_of::<(Uuid, String, String, String)>()
        );
    }
}
