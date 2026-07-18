use std::net::IpAddr;
use std::time::Duration;

use reqwest::Client;
use serde_json::json;
use uuid::Uuid;

use crate::model::ToolSource;
use crate::policy::ResolvedTool;
use crate::registry::{catalog, ToolExecutionCtx};

pub const MAX_TOOL_CALLS_PER_GENERATION: u8 = 5;
pub const BUILTIN_TIMEOUT_SECS: u64 = 10;
pub const TENANT_TIMEOUT_SECS: u64 = 15;
pub const MAX_RESPONSE_BYTES: u64 = 1_048_576; // 1 MiB

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationFailure {
    UnknownOrDisabled,
    SchemaMismatch(String),
}

pub fn validate(
    resolved: &[ResolvedTool],
    name: &str,
    arguments: &serde_json::Value,
) -> Result<ResolvedTool, ValidationFailure> {
    let tool = resolved
        .iter()
        .find(|t| t.spec.name == name)
        .ok_or(ValidationFailure::UnknownOrDisabled)?;

    let schema = &tool.spec.input_schema;

    if let Some(obj) = schema.as_object() {
        if let Some(required) = obj.get("required").and_then(|v| v.as_array()) {
            for req_field in required {
                let field_name = req_field.as_str().ok_or_else(|| {
                    ValidationFailure::SchemaMismatch("required field name not a string".into())
                })?;
                if !arguments.get(field_name).is_some() {
                    return Err(ValidationFailure::SchemaMismatch(format!(
                        "missing required field '{}'",
                        field_name
                    )));
                }
            }
        }

        if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
            for (key, value) in arguments.as_object().unwrap_or(&serde_json::Map::new()) {
                if let Some(prop) = properties.get(key) {
                    if let Some(expected_type) = prop.get("type").and_then(|v| v.as_str()) {
                        let actual_type = match value {
                            serde_json::Value::Null => "null",
                            serde_json::Value::Bool(_) => "boolean",
                            serde_json::Value::Number(_) => "number",
                            serde_json::Value::String(_) => "string",
                            serde_json::Value::Array(_) => "array",
                            serde_json::Value::Object(_) => "object",
                        };
                        if expected_type != actual_type {
                            return Err(ValidationFailure::SchemaMismatch(format!(
                                "field '{}' expected type '{}', got '{}'",
                                key, expected_type, actual_type
                            )));
                        }
                    }

                    if let Some(enum_values) = prop.get("enum").and_then(|v| v.as_array()) {
                        if !enum_values.iter().any(|ev| ev == value) {
                            return Err(ValidationFailure::SchemaMismatch(format!(
                                "field '{}' value '{}' is not in the allowed set",
                                key, value
                            )));
                        }
                    }
                }
            }
        }
    }

    Ok(tool.clone())
}

#[derive(Debug)]
pub enum ExecutionOutcome {
    Succeeded(serde_json::Value),
    Failed(String),
    TimedOut,
}

pub async fn execute(
    ctx: &ToolExecutionCtx,
    resolved: &ResolvedTool,
    arguments: serde_json::Value,
    tool_request_id: Uuid,
) -> ExecutionOutcome {
    match resolved.source {
        ToolSource::Builtin => {
            let tools_list = catalog();
            let builtin = match tools_list
                .into_iter()
                .find(|t| t.name() == resolved.spec.name)
            {
                Some(t) => t,
                None => {
                    return ExecutionOutcome::Failed(format!(
                        "builtin tool '{}' not found in catalog",
                        resolved.spec.name
                    ))
                }
            };

            match tokio::time::timeout(
                Duration::from_secs(BUILTIN_TIMEOUT_SECS),
                builtin.execute(ctx, arguments),
            )
            .await
            {
                Ok(Ok(result)) => ExecutionOutcome::Succeeded(result),
                Ok(Err(msg)) => ExecutionOutcome::Failed(ai_providers::sanitize_error_detail(&msg)),
                Err(_) => ExecutionOutcome::TimedOut,
            }
        }
        ToolSource::Tenant => {
            let tenant_tool_id = match resolved.tenant_tool_id {
                Some(id) => id,
                None => return ExecutionOutcome::Failed("tenant tool has no id".into()),
            };

            match execute_tenant_endpoint(
                ctx,
                tenant_tool_id,
                &resolved.spec.name,
                arguments,
                tool_request_id,
            )
            .await
            {
                Ok(result) => ExecutionOutcome::Succeeded(result),
                Err(e) => ExecutionOutcome::Failed(ai_providers::sanitize_error_detail(&e)),
            }
        }
    }
}

async fn execute_tenant_endpoint(
    ctx: &ToolExecutionCtx,
    tenant_tool_id: Uuid,
    tool_name: &str,
    arguments: serde_json::Value,
    tool_request_id: Uuid,
) -> Result<serde_json::Value, String> {
    let (tool, credential_ciphertext) = match crate::queries::fetch_tenant_tool_with_credential(
        &ctx.pool,
        ctx.tenant_id,
        tenant_tool_id,
    )
    .await
    {
        Ok(Some((t, c))) => (t, c),
        Ok(None) => return Err("tenant tool not found".into()),
        Err(e) => return Err(format!("failed to fetch tenant tool: {e}")),
    };

    // 1. Validate endpoint_url is https:// and not a private/loopback address
    let url = url_parse_and_validate(&tool.endpoint_url).await?;

    // 2. Build request body
    let body = json!({
        "tool": tool_name,
        "arguments": arguments,
        "conversation_id": ctx.conversation_id,
        "request_id": tool_request_id,
    });

    // 3. Build reqwest client and POST
    let client = Client::builder()
        .timeout(Duration::from_secs(TENANT_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let mut req = client.post(url).json(&body);

    // 4. Unseal credential into Authorization header
    if let Some(ciphertext_b64) = &credential_ciphertext {
        let master_key = ctx
            .master_key
            .as_ref()
            .ok_or_else(|| "no encryption key configured".to_string())?;

        // The credential is stored as base64(ciphertext)||base64(nonce)
        let parts: Vec<&str> = ciphertext_b64.split("||").collect();
        if parts.len() != 2 {
            return Err("invalid credential storage format".into());
        }
        let ct = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, parts[0])
            .map_err(|_| "failed to decode credential ciphertext".to_string())?;
        let nonce = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, parts[1])
            .map_err(|_| "failed to decode credential nonce".to_string())?;

        let scope = ai_providers::crypto::aad(Some(ctx.tenant_id), tool_name);
        let secret = ai_providers::crypto::open(master_key, &scope, &ct, &nonce)
            .map_err(|_| "failed to decrypt credential".to_string())?;

        req = req.header("Authorization", format!("Bearer {}", secret.expose()));
    }

    // 5. Send with outer timeout
    let mut response = tokio::time::timeout(Duration::from_secs(TENANT_TIMEOUT_SECS), req.send())
        .await
        .map_err(|_| "tenant tool request timed out".to_string())?
        .map_err(|e| {
            // Never log the credential
            format!("tenant tool request failed: {e}")
        })?;

    // 6. Check status and cap response body
    let status = response.status();

    if let Some(content_length) = response.content_length() {
        if content_length > MAX_RESPONSE_BYTES {
            return Err("response body exceeds maximum size".into());
        }
    }

    let mut body_bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("failed to read response chunk: {e}"))?
    {
        body_bytes.extend_from_slice(&chunk);
        if body_bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("response body exceeds maximum size".into());
        }
    }
    let bytes = body_bytes;

    if !status.is_success() {
        let body_text = String::from_utf8_lossy(&bytes);
        let truncated = &body_text[..body_text.len().min(200)];
        return Err(format!(
            "tenant tool returned {status}: {}",
            ai_providers::sanitize_error_detail(truncated)
        ));
    }

    // 7. Require JSON response body
    let result: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("response is not valid JSON: {e}"))?;

    Ok(result)
}

fn is_restricted(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return true;
            }
            // fc00::/7 — unique local (IPv6 private)
            let octets = v6.octets();
            if octets[0] & 0xfe == 0xfc {
                return true;
            }
            // fe80::/10 — IPv6 link-local
            octets[0] == 0xfe && octets[1] & 0xc0 == 0x80
        }
    }
}

pub async fn url_parse_and_validate(url_str: &str) -> Result<String, String> {
    let url = url::Url::parse(url_str).map_err(|e| format!("invalid endpoint URL: {e}"))?;

    if url.scheme() != "https" {
        return Err("endpoint URL must use https scheme".into());
    }

    let host = url
        .host_str()
        .ok_or_else(|| "endpoint URL has no host".to_string())?;

    // Resolve host to IP addresses and reject restricted ranges
    let addrs = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|_| "failed to resolve endpoint host".to_string())?;

    for addr in addrs {
        if is_restricted(addr.ip()) {
            return Err("endpoint URL must not resolve to a private or loopback address".into());
        }
    }

    Ok(url_str.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_providers::ToolSpec;
    use serde_json::json;

    fn make_resolved_tools() -> Vec<ResolvedTool> {
        vec![
            ResolvedTool {
                spec: ToolSpec {
                    name: "lookup_customer".into(),
                    description: "Look up customer profile".into(),
                    input_schema: json!({"type": "object", "properties": {}}),
                },
                source: ToolSource::Builtin,
                approval_required: false,
                tenant_tool_id: None,
            },
            ResolvedTool {
                spec: ToolSpec {
                    name: "update_customer_contact".into(),
                    description: "Update contact field".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "field": {"type": "string", "enum": ["email", "phone"]},
                            "value": {"type": "string"}
                        },
                        "required": ["field", "value"]
                    }),
                },
                source: ToolSource::Builtin,
                approval_required: true,
                tenant_tool_id: None,
            },
        ]
    }

    #[test]
    fn unknown_tool_returns_unknown_or_disabled() {
        let tools = make_resolved_tools();
        let result = validate(&tools, "nonexistent_tool", &json!({}));
        assert_eq!(result, Err(ValidationFailure::UnknownOrDisabled));
    }

    #[test]
    fn schema_mismatch_required_field() {
        let tools = make_resolved_tools();
        let result = validate(
            &tools,
            "update_customer_contact",
            &json!({"field": "email"}),
        );
        assert!(matches!(result, Err(ValidationFailure::SchemaMismatch(_))));
        let msg = match result.unwrap_err() {
            ValidationFailure::SchemaMismatch(m) => m,
            _ => unreachable!(),
        };
        assert!(msg.contains("required"));
    }

    #[test]
    fn schema_mismatch_type() {
        let tools = make_resolved_tools();
        let result = validate(
            &tools,
            "update_customer_contact",
            &json!({"field": "email", "value": 42}),
        );
        assert!(matches!(result, Err(ValidationFailure::SchemaMismatch(_))));
    }

    #[test]
    fn schema_mismatch_enum() {
        let tools = make_resolved_tools();
        let result = validate(
            &tools,
            "update_customer_contact",
            &json!({"field": "fax", "value": "test@example.com"}),
        );
        assert!(matches!(result, Err(ValidationFailure::SchemaMismatch(_))));
    }

    #[test]
    fn valid_request_succeeds() {
        let tools = make_resolved_tools();
        let result = validate(&tools, "lookup_customer", &json!({}));
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn execute_unknown_builtin_returns_failed() {
        let tool = ResolvedTool {
            spec: ToolSpec {
                name: "non_existent_tool".into(),
                description: "does not exist".into(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            source: ToolSource::Builtin,
            approval_required: false,
            tenant_tool_id: None,
        };

        let pool_opts = sqlx::postgres::PgPoolOptions::new().max_connections(1);
        let pool = pool_opts
            .connect_lazy("postgres://localhost:5432/nonexistent")
            .unwrap();
        let ctx = ToolExecutionCtx {
            tenant_id: uuid::Uuid::nil(),
            conversation_id: uuid::Uuid::nil(),
            pool,
            master_key: None,
        };
        // Pool is lazy and won't connect — tool isn't in the catalog anyway
        let result = execute(&ctx, &tool, json!({}), Uuid::nil()).await;
        assert!(matches!(result, ExecutionOutcome::Failed(_)));
    }
}
