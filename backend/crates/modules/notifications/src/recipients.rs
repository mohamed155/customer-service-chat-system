use crate::model::NotificationKind;
use authz::{tenant_role_permissions, Permission, TenantRole};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn roles_with_conversations_manage() -> Vec<String> {
    [
        TenantRole::Owner,
        TenantRole::Admin,
        TenantRole::Manager,
        TenantRole::Agent,
        TenantRole::Viewer,
    ]
    .into_iter()
    .filter(|r| tenant_role_permissions(*r).contains(&Permission::ConversationsManage))
    .map(|r| r.to_string())
    .collect()
}

pub async fn resolve(
    pool: &PgPool,
    tenant_id: Uuid,
    kind: &NotificationKind,
    subject_id: Uuid,
    actor_membership_id: Option<Uuid>,
    target_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if let Some(target) = target_membership_id {
        return resolve_target(pool, tenant_id, target, actor_membership_id).await;
    }

    match kind {
        NotificationKind::EscalationNew | NotificationKind::ToolApprovalRequired => {
            resolve_role_based(pool, tenant_id, &roles_with_conversations_manage(), actor_membership_id).await
        }
        NotificationKind::AiResponseFailed => {
            resolve_ai_failed(pool, tenant_id, subject_id, actor_membership_id).await
        }
        NotificationKind::ConversationAssigned => {
            Ok(Vec::new())
        }
    }
}

async fn resolve_target(
    pool: &PgPool,
    tenant_id: Uuid,
    target_membership_id: Uuid,
    actor_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id FROM tenant_memberships \
         WHERE id = $1 AND tenant_id = $2 AND status = 'active' AND deleted_at IS NULL \
         AND ($3::uuid IS NULL OR id IS DISTINCT FROM $3)",
    )
    .bind(target_membership_id)
    .bind(tenant_id)
    .bind(actor_membership_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.get("id")).collect())
}

async fn resolve_role_based(
    pool: &PgPool,
    tenant_id: Uuid,
    roles: &[String],
    actor_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if roles.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND role = ANY($2) AND status = 'active' AND deleted_at IS NULL \
         AND ($3::uuid IS NULL OR id IS DISTINCT FROM $3)",
    )
    .bind(tenant_id)
    .bind(roles)
    .bind(actor_membership_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.get("id")).collect())
}

async fn resolve_ai_failed(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    actor_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT tm.id FROM tenant_memberships tm \
         WHERE tm.tenant_id = $1 AND tm.status = 'active' AND tm.deleted_at IS NULL \
         AND ($3::uuid IS NULL OR tm.id IS DISTINCT FROM $3) \
         AND (tm.role IN ('owner', 'admin') \
              OR tm.id = (SELECT assigned_membership_id FROM conversations WHERE id = $2 AND assigned_membership_id IS NOT NULL))",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(actor_membership_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.get("id")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversations_manage_roles_include_owner_admin_manager_agent() {
        let roles = roles_with_conversations_manage();
        assert!(roles.contains(&"owner".to_string()));
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"manager".to_string()));
        assert!(roles.contains(&"agent".to_string()));
        assert!(!roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn roles_have_expected_exclusion() {
        let roles = roles_with_conversations_manage();
        assert_eq!(roles.len(), 4);
    }
}
