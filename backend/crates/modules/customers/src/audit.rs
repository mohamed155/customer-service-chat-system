use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RESOURCE_TYPE: &str = "customer";
const ACTION_CREATED: &str = "customer.created";
const ACTION_UPDATED: &str = "customer.updated";

pub async fn record_customer_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    customer_id: Uuid,
    created_fields: &[&str],
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_CREATED,
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&customer_id.to_string()),
        &details_for_created(created_fields),
    )
    .await
}

pub async fn record_customer_updated(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    customer_id: Uuid,
    changed_fields: &[&str],
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        ACTION_UPDATED,
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE,
        Some(&customer_id.to_string()),
        &details_for_updated(changed_fields),
    )
    .await
}

pub(crate) fn details_for_created(created_fields: &[&str]) -> Value {
    json!({
        "created_fields": created_fields,
    })
}

pub(crate) fn details_for_updated(changed_fields: &[&str]) -> Value {
    json!({ "changed_fields": changed_fields })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn details_for_created_is_an_empty_object() {
        assert_eq!(
            details_for_created(&[]),
            serde_json::json!({"created_fields": []})
        );
    }

    #[test]
    fn details_for_created_is_not_null() {
        assert!(
            details_for_created(&[]).is_object(),
            "customer.created details must be a JSON object, not null/array/primitive"
        );
    }

    #[test]
    fn details_for_created_lists_field_names() {
        let details = details_for_created(&["display_name", "email", "phone"]);
        let fields = details["created_fields"]
            .as_array()
            .expect("created_fields must be a JSON array");
        let names: Vec<&str> = fields
            .iter()
            .map(|v| v.as_str().expect("field name must be a string"))
            .collect();
        assert_eq!(names, vec!["display_name", "email", "phone"]);
    }

    #[test]
    fn details_for_updated_lists_changed_field_names_only() {
        let details = details_for_updated(&["email", "phone"]);
        let fields = details["changed_fields"]
            .as_array()
            .expect("changed_fields must be a JSON array");
        let names: Vec<&str> = fields
            .iter()
            .map(|v| v.as_str().expect("field name must be a JSON string"))
            .collect();
        assert_eq!(names, vec!["email", "phone"]);
    }

    #[test]
    fn details_for_updated_omits_unchanged_field_names() {
        let details = details_for_updated(&["display_name"]);
        let fields = details["changed_fields"].as_array().expect("array");
        let names: Vec<&str> = fields.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec!["display_name"]);
        assert!(
            !names.contains(&"phone"),
            "phone was not in changed_fields and must not appear"
        );
    }

    #[test]
    fn details_for_updated_records_field_names_no_values() {
        let details = details_for_updated(&["email", "identifiers", "metadata"]);
        let object = details.as_object().expect("details must be a JSON object");
        assert_eq!(object.len(), 1, "details must carry only changed_fields");
        assert!(object.contains_key("changed_fields"));
        let fields = object["changed_fields"].as_array().expect("array");
        for field in fields {
            let Value::String(name) = field else {
                panic!("each changed_field must be a string, got {field:?}");
            };
            assert!(
                !name.contains(':') && !name.contains('='),
                "field names must not carry values; got {name:?}"
            );
        }
    }

    #[test]
    fn details_for_updated_with_no_changes_records_empty_array() {
        let details = details_for_updated(&[]);
        let fields = details["changed_fields"]
            .as_array()
            .expect("changed_fields must be a JSON array even when empty");
        assert_eq!(fields.len(), 0);
    }

    #[test]
    fn details_for_updated_preserves_field_order() {
        let details = details_for_updated(&["display_name", "email", "identifiers", "metadata"]);
        let fields = details["changed_fields"].as_array().expect("array");
        let names: Vec<&str> = fields.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec!["display_name", "email", "identifiers", "metadata"],
            "field order must be preserved as supplied by the handler"
        );
    }
}
