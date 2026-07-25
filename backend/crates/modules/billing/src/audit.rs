use serde_json::json;
use sqlx::{Postgres, Transaction};

use crate::model;

pub async fn plan_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    plan: &model::PlanDto,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "billing_plan.created",
        Some(actor_user_id),
        None,
        "billing_plan",
        Some(&plan.id.to_string()),
        &json!({
            "name": &plan.name,
            "monthly_price_cents": plan.monthly_price_cents,
            "token_allowance": plan.token_allowance,
            "conversation_allowance": plan.conversation_allowance,
        }),
    )
    .await
}

pub async fn plan_status_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    plan_id: uuid::Uuid,
    from: &str,
    to: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "billing_plan.status_changed",
        Some(actor_user_id),
        None,
        "billing_plan",
        Some(&plan_id.to_string()),
        &json!({"from": from, "to": to}),
    )
    .await
}

pub async fn subscription_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    subscription_id: uuid::Uuid,
    before: &serde_json::Value,
    after: &serde_json::Value,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "billing_subscription.changed",
        Some(actor_user_id),
        Some(tenant_id),
        "billing_subscription",
        Some(&subscription_id.to_string()),
        &json!({
            "plan_id": after.get("plan_id"),
            "status": after.get("status"),
            "previous_plan_id": before.get("plan_id"),
            "previous_status": before.get("status"),
        }),
    )
    .await
}
