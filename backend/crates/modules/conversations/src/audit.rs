use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RESOURCE_TYPE: &str = "conversation";
const ACTION_CREATED: &str = "conversation.created";
const ACTION_STATUS_CHANGED: &str = "conversation.status_changed";
const ACTION_ASSIGNMENT_CHANGED: &str = "conversation.assignment_changed";

pub async fn record_conversation_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    conversation_id: Uuid,
    customer_id: Uuid,
    channel: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_CREATED,
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&conversation_id.to_string()),
        &details_for_created(customer_id, channel),
    )
    .await
}

pub async fn record_status_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    conversation_id: Uuid,
    from: &str,
    to: &str,
    auto: bool,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_STATUS_CHANGED,
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&conversation_id.to_string()),
        &details_for_status_changed(from, to, auto),
    )
    .await
}

pub async fn record_assignment_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    conversation_id: Uuid,
    from_membership_id: Option<Uuid>,
    to_membership_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_ASSIGNMENT_CHANGED,
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&conversation_id.to_string()),
        &details_for_assignment_changed(from_membership_id, to_membership_id),
    )
    .await
}

fn details_for_created(customer_id: Uuid, channel: &str) -> Value {
    json!({
        "customer_id": customer_id.to_string(),
        "channel": channel,
    })
}

fn details_for_status_changed(from: &str, to: &str, auto: bool) -> Value {
    json!({
        "from": from,
        "to": to,
        "auto": auto,
    })
}

fn details_for_assignment_changed(
    from_membership_id: Option<Uuid>,
    to_membership_id: Option<Uuid>,
) -> Value {
    json!({
        "from_membership_id": from_membership_id.map(|id| id.to_string()),
        "to_membership_id": to_membership_id.map(|id| id.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_for_created_contains_customer_id_and_channel() {
        let customer_id = Uuid::new_v4();
        let details = details_for_created(customer_id, "web");
        assert_eq!(details["customer_id"], customer_id.to_string());
        assert_eq!(details["channel"], "web");
        assert!(details.is_object());
    }

    #[test]
    fn details_for_status_changed_contains_from_to_auto() {
        let details = details_for_status_changed("open", "closed", true);
        assert_eq!(details["from"], "open");
        assert_eq!(details["to"], "closed");
        assert_eq!(details["auto"], true);
    }

    #[test]
    fn details_for_status_changed_auto_false() {
        let details = details_for_status_changed("open", "pending", false);
        assert_eq!(details["auto"], false);
    }

    #[test]
    fn details_for_assignment_changed_with_both_ids() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let details = details_for_assignment_changed(Some(from), Some(to));
        assert_eq!(details["from_membership_id"], from.to_string());
        assert_eq!(details["to_membership_id"], to.to_string());
    }

    #[test]
    fn details_for_assignment_changed_with_null_ids() {
        let details = details_for_assignment_changed(None, None);
        assert!(details["from_membership_id"].is_null());
        assert!(details["to_membership_id"].is_null());
    }

    #[test]
    fn details_for_assignment_changed_with_only_from() {
        let from = Uuid::new_v4();
        let details = details_for_assignment_changed(Some(from), None);
        assert_eq!(details["from_membership_id"], from.to_string());
        assert!(details["to_membership_id"].is_null());
    }

    #[test]
    fn details_for_assignment_changed_with_only_to() {
        let to = Uuid::new_v4();
        let details = details_for_assignment_changed(None, Some(to));
        assert!(details["from_membership_id"].is_null());
        assert_eq!(details["to_membership_id"], to.to_string());
    }
}
