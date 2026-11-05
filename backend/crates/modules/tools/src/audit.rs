use sqlx::PgPool;
use tracing::error;
use uuid::Uuid;

pub async fn record_execution(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    tool_name: &str,
    tool_request_id: Uuid,
    resource_id: &str,
    outcome: &str,
) {
    let details = serde_json::json!({
        "tool_name": tool_name,
        "conversation_id": conversation_id,
        "outcome": outcome,
        "request_id": tool_request_id,
    });

    let result = sqlx::query(
        "INSERT INTO audit_logs \
         (actor_user_id, action, resource_type, resource_id, tenant_id, details) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Option::<Uuid>::None)
    .bind("tool.executed")
    .bind("tenant_tool")
    .bind(resource_id)
    .bind(tenant_id)
    .bind(&details)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!(
            tool_name = %tool_name,
            tenant_id = %tenant_id,
            error = %e,
            "failed to record tool execution audit log"
        );
    }
}

pub async fn record_config_change(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_membership_id: Uuid,
    action: &str,
    details: serde_json::Value,
) {
    // Resolve user_id from membership
    let user_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM tenant_memberships WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(actor_membership_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    // audit_logs requires a resource_id (0013_audit_resource_required); a NULL
    // made every tool config audit fail. Tenant tools are identified by
    // `toolId`, built-in tool policies by `toolName`.
    let resource_id = details
        .get("toolId")
        .or_else(|| details.get("toolName"))
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_else(|| tenant_id.to_string());

    let result = sqlx::query(
        "INSERT INTO audit_logs \
         (actor_user_id, action, resource_type, resource_id, tenant_id, details) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(action)
    .bind("tenant_tool")
    .bind(resource_id)
    .bind(tenant_id)
    .bind(&details)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!(
            action = %action,
            tenant_id = %tenant_id,
            error = %e,
            "failed to record tool config change audit log"
        );
    }
}
