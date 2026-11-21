use sqlx::PgPool;
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

    // Best effort, in a transaction of its own: the tool has already run, and nothing has to stay
    // consistent with this record.
    audit_log::record(
        pool,
        "tool.executed",
        None,
        Some(tenant_id),
        "tenant_tool",
        Some(resource_id),
        &details,
    )
    .await;
}

pub async fn record_config_change(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_membership_id: Uuid,
    action: &str,
    details: serde_json::Value,
) {
    // Resolve user_id from membership
    let user_id: Option<Uuid> = match db::UnitOfWork::begin_for_tenant(pool, tenant_id).await {
        Ok(mut uow) => {
            tenancy::directory::membership_user_id(uow.conn(), tenant_id, actor_membership_id)
                .await
                .unwrap_or(None)
        }
        Err(_) => None,
    };

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

    audit_log::record(
        pool,
        action,
        user_id,
        Some(tenant_id),
        "tenant_tool",
        Some(&resource_id),
        &details,
    )
    .await;
}
