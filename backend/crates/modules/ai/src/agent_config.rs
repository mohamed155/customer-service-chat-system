use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Postgres;
use sqlx::Transaction;
use utoipa::ToSchema;
use uuid::Uuid;

pub const TONES: [&str; 5] = ["professional", "friendly", "casual", "formal", "empathetic"];

pub const CATALOG_CHANNELS: [&str; 5] = ["email", "phone", "web_chat", "whatsapp", "telegram"];

pub const PROVIDER_CATALOG: [&str; 3] = ["openai", "anthropic", "gemini"];

pub const AVATAR_PRESETS: &[&str] = &["spark", "orbit", "beacon", "nova", "pulse", "atlas"];

pub const CURATED_MODELS: &[(&str, &[&str])] = &[
    ("openai", &["gpt-4.1", "gpt-4.1-mini"]),
    ("anthropic", &["claude-sonnet-5", "claude-haiku-4-5"]),
    ("gemini", &["gemini-2.5-pro", "gemini-2.5-flash"]),
];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentConfigurationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub is_default: bool,
    pub avatar_kind: String,
    pub avatar_preset: Option<String>,
    pub tone: String,
    pub system_prompt: String,
    pub business_rules: serde_json::Value,
    pub escalation_rules: serde_json::Value,
    pub enabled_channels: serde_json::Value,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EscalationTrigger {
    HumanRequest,
    TopicKeywords,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalationRule {
    pub id: Uuid,
    pub name: String,
    pub trigger: EscalationTrigger,
    pub keywords: Vec<String>,
    pub required_skill_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvatarPayload {
    pub kind: String,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalationRulePayload {
    pub id: Option<Uuid>,
    pub name: String,
    pub trigger: EscalationTrigger,
    pub keywords: Vec<String>,
    pub required_skill_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSelectionPayload {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigPayload {
    pub name: String,
    pub avatar: AvatarPayload,
    pub tone: String,
    pub system_prompt: String,
    pub business_rules: Vec<String>,
    pub escalation_rules: Vec<EscalationRulePayload>,
    pub enabled_channels: Vec<String>,
    pub provider_selection: Option<ProviderSelectionPayload>,
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}

pub fn validate_payload(payload: &AgentConfigPayload) -> Result<(), Vec<ValidationIssue>> {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    let trimmed_name = payload.name.trim();
    if trimmed_name.is_empty() || trimmed_name.len() > 80 {
        issues.push(ValidationIssue {
            field: "name".into(),
            code: "invalid_length".into(),
            message: "name must be between 1 and 80 characters".into(),
        });
    }

    if !TONES.contains(&payload.tone.as_str()) {
        issues.push(ValidationIssue {
            field: "tone".into(),
            code: "invalid_value".into(),
            message: format!("tone must be one of: {}", TONES.join(", ")),
        });
    }

    if payload.system_prompt.len() > 8000 {
        issues.push(ValidationIssue {
            field: "systemPrompt".into(),
            code: "too_long".into(),
            message: "system prompt must not exceed 8000 characters".into(),
        });
    }

    if payload.business_rules.len() > 20 {
        issues.push(ValidationIssue {
            field: "businessRules".into(),
            code: "too_many".into(),
            message: "at most 20 business rules allowed".into(),
        });
    }
    for (i, rule) in payload.business_rules.iter().enumerate() {
        let trimmed = rule.trim();
        if trimmed.is_empty() || trimmed.len() > 500 {
            issues.push(ValidationIssue {
                field: format!("businessRules[{}]", i),
                code: "invalid_length".into(),
                message: "each business rule must be between 1 and 500 characters".into(),
            });
        }
    }

    if payload.escalation_rules.len() > 20 {
        issues.push(ValidationIssue {
            field: "escalationRules".into(),
            code: "too_many".into(),
            message: "at most 20 escalation rules allowed".into(),
        });
    }
    let mut rule_names: Vec<String> = Vec::new();
    for (i, rule) in payload.escalation_rules.iter().enumerate() {
        let prefix = format!("escalationRules[{}]", i);
        let trimmed_name = rule.name.trim();
        if trimmed_name.is_empty() || trimmed_name.len() > 80 {
            issues.push(ValidationIssue {
                field: format!("{}.name", prefix),
                code: "invalid_length".into(),
                message: "rule name must be between 1 and 80 characters".into(),
            });
        }
        let lower_name = rule.name.to_lowercase();
        if rule_names.contains(&lower_name) {
            issues.push(ValidationIssue {
                field: format!("{}.name", prefix),
                code: "duplicate".into(),
                message: "rule names must be unique (case-insensitive)".into(),
            });
        }
        rule_names.push(lower_name);

        match &rule.trigger {
            EscalationTrigger::TopicKeywords => {
                if rule.keywords.is_empty() {
                    issues.push(ValidationIssue {
                        field: format!("{}.keywords", prefix),
                        code: "required".into(),
                        message: "keywords are required for topic_keywords trigger".into(),
                    });
                }
                for (j, kw) in rule.keywords.iter().enumerate() {
                    if kw.trim().is_empty() || kw.len() > 40 {
                        issues.push(ValidationIssue {
                            field: format!("{}.keywords[{}]", prefix, j),
                            code: "invalid_length".into(),
                            message: "each keyword must be between 1 and 40 characters".into(),
                        });
                    }
                }
            }
            EscalationTrigger::HumanRequest => {
                if !rule.keywords.is_empty() {
                    issues.push(ValidationIssue {
                        field: format!("{}.keywords", prefix),
                        code: "must_be_empty".into(),
                        message: "keywords must be empty for human_request trigger".into(),
                    });
                }
            }
        }
    }

    let mut seen_channels: Vec<String> = Vec::new();
    for (i, channel) in payload.enabled_channels.iter().enumerate() {
        if !CATALOG_CHANNELS.contains(&channel.as_str()) {
            issues.push(ValidationIssue {
                field: format!("enabled_channels[{}]", i),
                code: "invalid_value".into(),
                message: format!("'{}' is not a valid channel", channel),
            });
        }
        if seen_channels.contains(channel) {
            issues.push(ValidationIssue {
                field: format!("enabled_channels[{}]", i),
                code: "duplicate".into(),
                message: "duplicate channel".into(),
            });
        }
        seen_channels.push(channel.clone());
    }

    if let Some(ref sel) = payload.provider_selection {
        if !PROVIDER_CATALOG.contains(&sel.provider.as_str()) {
            issues.push(ValidationIssue {
                field: "providerSelection.provider".into(),
                code: "invalid_value".into(),
                message: format!("provider must be one of: {}", PROVIDER_CATALOG.join(", ")),
            });
        }
        if sel.model.trim().is_empty() {
            issues.push(ValidationIssue {
                field: "providerSelection.model".into(),
                code: "required".into(),
                message: "model must not be empty".into(),
            });
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

pub async fn load_live(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Option<AgentConfigurationRow>> {
    sqlx::query_as::<_, AgentConfigurationRow>(
        "SELECT * FROM agent_configurations WHERE tenant_id = $1 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

pub async fn load_live_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<Option<AgentConfigurationRow>> {
    sqlx::query_as::<_, AgentConfigurationRow>(
        "SELECT * FROM agent_configurations WHERE tenant_id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn create_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    payload: &AgentConfigPayload,
) -> sqlx::Result<AgentConfigurationRow> {
    sqlx::query_as::<_, AgentConfigurationRow>(
        "INSERT INTO agent_configurations \
         (tenant_id, name, is_default, avatar_kind, avatar_preset, tone, system_prompt, \
          business_rules, escalation_rules, enabled_channels, provider, model, version) \
         VALUES ($1, $2, true, $3, $4, $5, $6, $7, $8, $9, $10, $11, 1) \
         RETURNING *",
    )
    .bind(tenant_id)
    .bind(&payload.name)
    .bind(&payload.avatar.kind)
    .bind(&payload.avatar.preset)
    .bind(&payload.tone)
    .bind(&payload.system_prompt)
    .bind(serde_json::to_value(&payload.business_rules).unwrap_or_default())
    .bind(serde_json::to_value(&payload.escalation_rules).unwrap_or_default())
    .bind(serde_json::to_value(&payload.enabled_channels).unwrap_or_default())
    .bind(payload.provider_selection.as_ref().map(|p| &p.provider))
    .bind(payload.provider_selection.as_ref().map(|p| &p.model))
    .fetch_one(&mut **tx)
    .await
}

pub async fn update_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    agent_id: Uuid,
    expected_version: i32,
    payload: &AgentConfigPayload,
) -> sqlx::Result<Option<AgentConfigurationRow>> {
    sqlx::query_as::<_, AgentConfigurationRow>(
        "UPDATE agent_configurations \
         SET name = $1, avatar_kind = $2, avatar_preset = $3, tone = $4, system_prompt = $5, \
             business_rules = $6, escalation_rules = $7, enabled_channels = $8, \
             provider = $9, model = $10, version = version + 1 \
         WHERE tenant_id = $11 AND id = $12 AND version = $13 AND deleted_at IS NULL \
         RETURNING *",
    )
    .bind(&payload.name)
    .bind(&payload.avatar.kind)
    .bind(&payload.avatar.preset)
    .bind(&payload.tone)
    .bind(&payload.system_prompt)
    .bind(serde_json::to_value(&payload.business_rules).unwrap_or_default())
    .bind(serde_json::to_value(&payload.escalation_rules).unwrap_or_default())
    .bind(serde_json::to_value(&payload.enabled_channels).unwrap_or_default())
    .bind(payload.provider_selection.as_ref().map(|p| &p.provider))
    .bind(payload.provider_selection.as_ref().map(|p| &p.model))
    .bind(tenant_id)
    .bind(agent_id)
    .bind(expected_version)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn live_skill_ids(
    pool: &PgPool,
    tenant_id: Uuid,
    ids: &[Uuid],
) -> sqlx::Result<Vec<Uuid>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM skills WHERE tenant_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

pub async fn agent_exists(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_configurations WHERE tenant_id = $1 AND deleted_at IS NULL)",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
}

pub struct AiAgentStatusAdapter {
    pub pool: PgPool,
}

#[async_trait::async_trait]
impl conversations::AiAgentStatus for AiAgentStatusAdapter {
    async fn agent_configured(&self, tenant_id: Uuid) -> bool {
        agent_exists(&self.pool, tenant_id).await.unwrap_or(false)
    }

    async fn platform_ai_available(&self, tenant_id: Uuid) -> bool {
        let resolved = crate::resolution::resolve_config(
            &self.pool,
            crate::resolution::Scope::Tenant(tenant_id),
        )
        .await
        .ok()
        .flatten();
        match resolved {
            Some(cfg) => credential_resolves(&self.pool, tenant_id, &cfg.row.provider).await,
            None => false,
        }
    }
}

pub async fn credential_resolves(pool: &PgPool, tenant_id: Uuid, provider: &str) -> bool {
    crate::resolution::resolve_credential_view(
        pool,
        crate::resolution::Scope::Tenant(tenant_id),
        provider,
    )
    .await
    .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_payload(name: &str, tone: &str, prompt: &str) -> AgentConfigPayload {
        AgentConfigPayload {
            name: name.into(),
            avatar: AvatarPayload {
                kind: "preset".into(),
                preset: Some("spark".into()),
            },
            tone: tone.into(),
            system_prompt: prompt.into(),
            business_rules: vec![],
            escalation_rules: vec![],
            enabled_channels: vec!["web_chat".into()],
            provider_selection: None,
            version: None,
        }
    }

    #[test]
    fn empty_name_fails() {
        let payload = make_payload("   ", "professional", "");
        let result = validate_payload(&payload);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.field == "name"));
    }

    #[test]
    fn name_81_chars_fails() {
        let long_name = "a".repeat(81);
        let payload = make_payload(&long_name, "professional", "");
        let result = validate_payload(&payload);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.field == "name"));
    }

    #[test]
    fn name_80_chars_passes() {
        let name_80 = "a".repeat(80);
        let payload = make_payload(&name_80, "professional", "");
        let result = validate_payload(&payload);
        assert!(result.is_ok());
    }

    #[test]
    fn invalid_tone_fails() {
        let payload = make_payload("test agent", "nonexistent_tone", "");
        let result = validate_payload(&payload);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.field == "tone"));
    }

    #[test]
    fn system_prompt_8001_fails() {
        let long_prompt = "x".repeat(8001);
        let payload = make_payload("test agent", "professional", &long_prompt);
        let result = validate_payload(&payload);
        assert!(result.is_err());
        let issues = result.unwrap_err();
        assert!(issues.iter().any(|i| i.field == "systemPrompt"));
    }

    #[test]
    fn minimal_payload_passes() {
        let payload = make_payload("My Agent", "casual", "");
        let result = validate_payload(&payload);
        assert!(result.is_ok());
    }
}
