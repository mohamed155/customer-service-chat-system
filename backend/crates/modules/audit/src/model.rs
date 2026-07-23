use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditActorDto {
    pub kind: String,
    pub id: Option<Uuid>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_platform_staff: bool,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditEntryDto {
    pub id: Uuid,
    pub action: String,
    pub category: String,
    pub actor: AuditActorDto,
    pub resource_type: String,
    pub resource_id: String,
    pub tenant_id: Option<Uuid>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditPagination {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditListResponse {
    pub data: Vec<AuditEntryDto>,
    pub pagination: AuditPagination,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub category: Option<String>,
    pub actor_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
}

pub const CATEGORY_PREFIXES: &[(&str, &[&str])] = &[
    ("auth", &["auth."]),
    ("tenant", &["platform.", "tenant."]),
    ("members", &["member.", "skill.", "availability."]),
    ("prompts", &["agent_prompt."]),
    ("ai", &["ai_config.", "ai_credential.", "agent_config."]),
    ("tools", &["tool."]),
    ("billing", &["billing."]),
    ("conversations", &["conversation."]),
    ("customers", &["customer."]),
    ("escalations", &["escalation."]),
    ("knowledge", &["knowledge_category.", "knowledge_document.", "knowledge_item."]),
    ("widgets", &["widget_instance."]),
    ("integrations", &["integration."]),
];

pub fn category_for_action(action: &str) -> &'static str {
    let mut best: &'static str = "other";
    let mut best_len: usize = 0;
    for &(category, prefixes) in CATEGORY_PREFIXES {
        for prefix in prefixes {
            if action.starts_with(prefix) && prefix.len() > best_len {
                best = category;
                best_len = prefix.len();
            }
        }
    }
    best
}

pub fn prefixes_for_category(category: &str) -> Option<Vec<String>> {
    CATEGORY_PREFIXES
        .iter()
        .find(|&&(cat, _)| cat == category)
        .map(|&(_, prefixes)| prefixes.iter().map(|p| format!("{}%", p)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_for_action_member_role_changed() {
        assert_eq!(category_for_action("member.role_changed"), "members");
    }

    #[test]
    fn category_for_action_agent_prompt_version_created() {
        assert_eq!(category_for_action("agent_prompt.version_created"), "prompts");
    }

    #[test]
    fn category_for_action_unknown() {
        assert_eq!(category_for_action("zzz.unknown"), "other");
    }

    #[test]
    fn prefixes_for_category_unknown_is_none() {
        assert!(prefixes_for_category("nope").is_none());
    }

    #[test]
    fn prefixes_for_category_members() {
        let prefixes = prefixes_for_category("members").unwrap();
        assert_eq!(prefixes, vec!["member.%", "skill.%", "availability.%"]);
    }
}
