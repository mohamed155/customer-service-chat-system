use serde_json::json;
use uuid::Uuid;

use crate::model::Classification;

pub struct ToolExecutionCtx {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub pool: sqlx::PgPool,
    pub master_key: Option<ai_providers::crypto::MasterKey>,
}

#[async_trait::async_trait]
pub trait BuiltinTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn spec(&self) -> ai_providers::ToolSpec;
    fn classification(&self) -> Classification;
    async fn execute(
        &self,
        ctx: &ToolExecutionCtx,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
}

pub struct LookupCustomer;

#[async_trait::async_trait]
impl BuiltinTool for LookupCustomer {
    fn name(&self) -> &'static str {
        "lookup_customer"
    }

    fn spec(&self) -> ai_providers::ToolSpec {
        ai_providers::ToolSpec {
            name: self.name().to_string(),
            description: "Look up the current customer's profile by conversation context"
                .to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
        }
    }

    fn classification(&self) -> Classification {
        Classification::Auto
    }

    async fn execute(
        &self,
        ctx: &ToolExecutionCtx,
        _args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let customer_id = conversations::queries::customer_id_for_conversation(
            &ctx.pool,
            ctx.tenant_id,
            ctx.conversation_id,
        )
        .await
        .map_err(|e| format!("failed to resolve conversation customer: {e}"))?;

        let profile =
            customers::queries::fetch_profile_for_tool(&ctx.pool, ctx.tenant_id, customer_id)
                .await
                .map_err(|e| format!("failed to fetch customer profile: {e}"))?
                .ok_or_else(|| "customer not found".to_string())?;

        Ok(json!({
            "display_name": profile.display_name,
            "email": profile.email,
            "phone": profile.phone,
            "conversation_count": profile.conversation_count,
        }))
    }
}

pub struct UpdateCustomerContact;

#[async_trait::async_trait]
impl BuiltinTool for UpdateCustomerContact {
    fn name(&self) -> &'static str {
        "update_customer_contact"
    }

    fn spec(&self) -> ai_providers::ToolSpec {
        ai_providers::ToolSpec {
            name: self.name().to_string(),
            description: "Update a customer's email or phone contact field".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "field": {
                        "type": "string",
                        "enum": ["email", "phone"]
                    },
                    "value": {
                        "type": "string"
                    }
                },
                "required": ["field", "value"]
            }),
        }
    }

    fn classification(&self) -> Classification {
        Classification::Approval
    }

    async fn execute(
        &self,
        ctx: &ToolExecutionCtx,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let field = args
            .get("field")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing 'field' argument".to_string())?;
        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "missing 'value' argument".to_string())?;

        let customer_id = conversations::queries::customer_id_for_conversation(
            &ctx.pool,
            ctx.tenant_id,
            ctx.conversation_id,
        )
        .await
        .map_err(|e| format!("failed to resolve conversation customer: {e}"))?;

        let mut tx = ctx
            .pool
            .begin()
            .await
            .map_err(|e| format!("failed to begin transaction: {e}"))?;

        let updated = customers::queries::update_contact_field_in_tx(
            &mut tx,
            ctx.tenant_id,
            customer_id,
            field,
            value,
        )
        .await
        .map_err(|e| format!("failed to update contact field: {e}"))?;

        tx.commit()
            .await
            .map_err(|e| format!("failed to commit transaction: {e}"))?;

        Ok(json!({
            "updated": updated,
            "field": field,
        }))
    }
}

pub fn catalog() -> Vec<Box<dyn BuiltinTool>> {
    vec![Box::new(LookupCustomer), Box::new(UpdateCustomerContact)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_returns_expected_tools() {
        let tools = catalog();
        assert_eq!(tools.len(), 2);

        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"lookup_customer"));
        assert!(names.contains(&"update_customer_contact"));

        let lookup = tools
            .iter()
            .find(|t| t.name() == "lookup_customer")
            .unwrap();
        assert!(matches!(lookup.classification(), Classification::Auto));

        let update = tools
            .iter()
            .find(|t| t.name() == "update_customer_contact")
            .unwrap();
        assert!(matches!(update.classification(), Classification::Approval));
    }
}
