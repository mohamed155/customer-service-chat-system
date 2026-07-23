use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    NotConnected,
    Connected,
    Error,
    Disconnected,
}

impl ConnectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotConnected => "not_connected",
            Self::Connected => "connected",
            Self::Error => "error",
            Self::Disconnected => "disconnected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Connected,
    ConfigUpdated,
    SecretRotated,
    Disconnected,
    DeliveryAccepted,
    DeliveryRejected,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::ConfigUpdated => "config_updated",
            Self::SecretRotated => "secret_rotated",
            Self::Disconnected => "disconnected",
            Self::DeliveryAccepted => "delivery_accepted",
            Self::DeliveryRejected => "delivery_rejected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RejectionReason {
    InvalidSignature,
    InactiveConnection,
    PayloadTooLarge,
    RateLimited,
    MalformedPayload,
}

impl RejectionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidSignature => "invalid_signature",
            Self::InactiveConnection => "inactive_connection",
            Self::PayloadTooLarge => "payload_too_large",
            Self::RateLimited => "rate_limited",
            Self::MalformedPayload => "malformed_payload",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfigFieldDto {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationListItemDto {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub is_available: bool,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationListResponse {
    pub data: Vec<IntegrationListItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationSecretRefDto {
    pub field_key: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationConnectionDto {
    pub config: Map<String, Value>,
    pub secrets: Vec<IntegrationSecretRefDto>,
    pub webhook_url: Option<String>,
    pub connected_at: DateTime<Utc>,
    pub disconnected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationDetailDto {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub is_available: bool,
    pub status: ConnectionStatus,
    pub config_schema: Vec<ConfigFieldDto>,
    pub connection: Option<IntegrationConnectionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationEventDto {
    pub id: Uuid,
    pub event_type: EventType,
    pub outcome: String,
    pub reason: Option<RejectionReason>,
    pub actor_membership_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginationInfo {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IntegrationEventListResponse {
    pub data: Vec<IntegrationEventDto>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConnectPayload {
    pub config: Map<String, Value>,
    pub secrets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateConfigPayload {
    pub config: Map<String, Value>,
    #[serde(default)]
    pub secrets: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventsQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

pub fn validate_against_schema(
    schema: &[ConfigFieldDto],
    config: &Map<String, Value>,
    secrets: &BTreeMap<String, String>,
    require_all: bool,
) -> Result<(), Vec<serde_json::Value>> {
    let mut errors: Vec<serde_json::Value> = Vec::new();

    for field in schema {
        let in_config = config.contains_key(&field.key);
        let in_secrets = secrets.contains_key(&field.key);

        if require_all && field.required && !in_config && !in_secrets {
            errors.push(serde_json::json!({
                "field": field.key,
                "message": "required field is missing",
            }));
            continue;
        }

        if field.kind == "secret" && in_config {
            errors.push(serde_json::json!({
                "field": field.key,
                "message": "secret fields must not be submitted as config",
            }));
        }
        if field.kind == "text" && in_secrets {
            errors.push(serde_json::json!({
                "field": field.key,
                "message": "text fields must not be submitted as secrets",
            }));
        }

        if field.kind == "text" && in_config {
            if let Some(value) = config.get(&field.key) {
                if value.is_null() {
                    if field.required {
                        errors.push(serde_json::json!({
                            "field": field.key,
                            "message": "value must not be null",
                        }));
                    }
                } else if let Some(s) = value.as_str() {
                    if field.required && s.is_empty() {
                        errors.push(serde_json::json!({
                            "field": field.key,
                            "message": "value must not be empty",
                        }));
                    }
                }
            }
        }

        if field.kind == "secret" && in_secrets {
            if let Some(s) = secrets.get(&field.key) {
                if s.is_empty() {
                    errors.push(serde_json::json!({
                        "field": field.key,
                        "message": "secret value must not be empty",
                    }));
                }
            }
        }
    }

    for key in config.keys() {
        if !schema.iter().any(|f| f.key == *key) {
            errors.push(serde_json::json!({
                "field": key,
                "message": "unknown config key",
            }));
        }
    }
    for key in secrets.keys() {
        if !schema.iter().any(|f| f.key == *key) {
            errors.push(serde_json::json!({
                "field": key,
                "message": "unknown secret key",
            }));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Vec<ConfigFieldDto> {
        vec![
            ConfigFieldDto {
                key: "source_label".to_string(),
                label: "Source label".to_string(),
                kind: "text".to_string(),
                required: true,
            },
            ConfigFieldDto {
                key: "signing_secret".to_string(),
                label: "Signing secret".to_string(),
                kind: "secret".to_string(),
                required: true,
            },
        ]
    }

    fn ok_config() -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("source_label".to_string(), json!("Billing"));
        m
    }
    fn ok_secrets() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert("signing_secret".to_string(), "whsec_xyz".to_string());
        m
    }

    #[test]
    fn accepts_complete_payload() {
        assert!(validate_against_schema(&schema(), &ok_config(), &ok_secrets(), true).is_ok());
    }

    #[test]
    fn rejects_missing_required_text_field() {
        let config = Map::new();
        let errs = validate_against_schema(&schema(), &config, &ok_secrets(), true).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "source_label"));
    }

    #[test]
    fn rejects_missing_required_secret_field() {
        let secrets = BTreeMap::new();
        let errs = validate_against_schema(&schema(), &ok_config(), &secrets, true).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "signing_secret"));
    }

    #[test]
    fn rejects_secret_submitted_in_config() {
        let mut config = ok_config();
        config.insert(
            "signing_secret".to_string(),
            json!("leakable_plaintext"),
        );
        let errs =
            validate_against_schema(&schema(), &config, &BTreeMap::new(), false).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "signing_secret"));
    }

    #[test]
    fn rejects_text_submitted_in_secrets() {
        let mut secrets = ok_secrets();
        secrets.insert("source_label".to_string(), "wrong_bucket".to_string());
        let errs =
            validate_against_schema(&schema(), &Map::new(), &secrets, false).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "source_label"));
    }

    #[test]
    fn rejects_unknown_keys() {
        let mut config = ok_config();
        config.insert("nonsense".to_string(), json!("x"));
        let errs =
            validate_against_schema(&schema(), &config, &ok_secrets(), true).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "nonsense"));
    }

    #[test]
    fn rejects_empty_required_text_field() {
        let mut config = Map::new();
        config.insert("source_label".to_string(), json!(""));
        let errs =
            validate_against_schema(&schema(), &config, &ok_secrets(), true).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "source_label"));
    }

    #[test]
    fn rejects_empty_required_secret_field() {
        let mut secrets = BTreeMap::new();
        secrets.insert("signing_secret".to_string(), String::new());
        let errs = validate_against_schema(&schema(), &ok_config(), &secrets, true).unwrap_err();
        assert!(errs.iter().any(|e| e["field"] == "signing_secret"));
    }

    #[test]
    fn require_all_false_allows_partial() {
        let config = Map::new();
        let secrets = BTreeMap::new();
        assert!(validate_against_schema(&schema(), &config, &secrets, false).is_ok());
    }
}
