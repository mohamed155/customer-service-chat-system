use crate::model::NotificationKind;
use authz::{tenant_role_permissions, Permission, TenantRole};
use sqlx::PgPool;
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
            resolve_role_based(
                pool,
                tenant_id,
                &roles_with_conversations_manage(),
                actor_membership_id,
            )
            .await
        }
        NotificationKind::AiResponseFailed => {
            resolve_ai_failed(pool, tenant_id, subject_id, actor_membership_id).await
        }
        NotificationKind::ConversationAssigned => Ok(Vec::new()),
    }
}

async fn resolve_target(
    pool: &PgPool,
    tenant_id: Uuid,
    target_membership_id: Uuid,
    actor_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: Result<Vec<Uuid>, sqlx::Error> = async {
        tenancy::directory::active_member_ids(
            uow.conn(),
            tenant_id,
            tenancy::directory::ActiveMembers {
                or_id: Some(target_membership_id),
                exclude: actor_membership_id,
                ..Default::default()
            },
        )
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

async fn resolve_role_based(
    pool: &PgPool,
    tenant_id: Uuid,
    roles: &[String],
    actor_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: Result<Vec<Uuid>, sqlx::Error> = async {
        if roles.is_empty() {
            return Ok(Vec::new());
        }

        let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
        tenancy::directory::active_member_ids(
            uow.conn(),
            tenant_id,
            tenancy::directory::ActiveMembers {
                roles: Some(&roles),
                exclude: actor_membership_id,
                ..Default::default()
            },
        )
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

async fn resolve_ai_failed(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    actor_membership_id: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: Result<Vec<Uuid>, sqlx::Error> = async {
        // The owners and admins, and whoever the conversation is assigned to.
        let assigned: Option<Uuid> = sqlx::query_scalar(
            "SELECT assigned_membership_id FROM conversations WHERE id = $1 AND tenant_id = $2",
        )
        .bind(conversation_id)
        .bind(tenant_id)
        .fetch_optional(uow.conn())
        .await?
        .flatten();
        tenancy::directory::active_member_ids(
            uow.conn(),
            tenant_id,
            tenancy::directory::ActiveMembers {
                roles: Some(&["owner", "admin"]),
                or_id: assigned,
                exclude: actor_membership_id,
                ..Default::default()
            },
        )
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
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
