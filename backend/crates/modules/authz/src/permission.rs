use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// An operation that may be granted by an authorization role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    #[serde(rename = "overview.view")]
    OverviewView,
    #[serde(rename = "conversations.view")]
    ConversationsView,
    #[serde(rename = "conversations.manage")]
    ConversationsManage,
    #[serde(rename = "customers.view")]
    CustomersView,
    #[serde(rename = "customers.manage")]
    CustomersManage,
    #[serde(rename = "ai_agent.view")]
    AiAgentView,
    #[serde(rename = "ai_agent.manage")]
    AiAgentManage,
    #[serde(rename = "knowledge_base.view")]
    KnowledgeBaseView,
    #[serde(rename = "knowledge_base.manage")]
    KnowledgeBaseManage,
    #[serde(rename = "integrations.view")]
    IntegrationsView,
    #[serde(rename = "integrations.manage")]
    IntegrationsManage,
    #[serde(rename = "analytics.view")]
    AnalyticsView,
    #[serde(rename = "members.view")]
    MembersView,
    #[serde(rename = "members.manage")]
    MembersManage,
    #[serde(rename = "settings.view")]
    SettingsView,
    #[serde(rename = "settings.manage")]
    SettingsManage,
    #[serde(rename = "billing.view")]
    BillingView,
    #[serde(rename = "billing.manage")]
    BillingManage,
    #[serde(rename = "tenant.delete")]
    TenantDelete,
    #[serde(rename = "owner.assign")]
    OwnerAssign,
    #[serde(rename = "platform.tenants.list")]
    PlatformTenantsList,
    #[serde(rename = "platform.tenants.switch")]
    PlatformTenantsSwitch,
    #[serde(rename = "platform.admin")]
    PlatformAdmin,
    #[serde(rename = "platform.billing.view")]
    PlatformBillingView,
    #[serde(rename = "platform.diagnostics.view")]
    PlatformDiagnosticsView,
}

impl Permission {
    pub const TENANT: [Self; 20] = [
        Self::OverviewView,
        Self::ConversationsView,
        Self::ConversationsManage,
        Self::CustomersView,
        Self::CustomersManage,
        Self::AiAgentView,
        Self::AiAgentManage,
        Self::KnowledgeBaseView,
        Self::KnowledgeBaseManage,
        Self::IntegrationsView,
        Self::IntegrationsManage,
        Self::AnalyticsView,
        Self::MembersView,
        Self::MembersManage,
        Self::SettingsView,
        Self::SettingsManage,
        Self::BillingView,
        Self::BillingManage,
        Self::TenantDelete,
        Self::OwnerAssign,
    ];

    pub const ALL: [Self; 25] = [
        Self::OverviewView,
        Self::ConversationsView,
        Self::ConversationsManage,
        Self::CustomersView,
        Self::CustomersManage,
        Self::AiAgentView,
        Self::AiAgentManage,
        Self::KnowledgeBaseView,
        Self::KnowledgeBaseManage,
        Self::IntegrationsView,
        Self::IntegrationsManage,
        Self::AnalyticsView,
        Self::MembersView,
        Self::MembersManage,
        Self::SettingsView,
        Self::SettingsManage,
        Self::BillingView,
        Self::BillingManage,
        Self::TenantDelete,
        Self::OwnerAssign,
        Self::PlatformTenantsList,
        Self::PlatformTenantsSwitch,
        Self::PlatformAdmin,
        Self::PlatformBillingView,
        Self::PlatformDiagnosticsView,
    ];
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self {
            Self::OverviewView => "overview.view",
            Self::ConversationsView => "conversations.view",
            Self::ConversationsManage => "conversations.manage",
            Self::CustomersView => "customers.view",
            Self::CustomersManage => "customers.manage",
            Self::AiAgentView => "ai_agent.view",
            Self::AiAgentManage => "ai_agent.manage",
            Self::KnowledgeBaseView => "knowledge_base.view",
            Self::KnowledgeBaseManage => "knowledge_base.manage",
            Self::IntegrationsView => "integrations.view",
            Self::IntegrationsManage => "integrations.manage",
            Self::AnalyticsView => "analytics.view",
            Self::MembersView => "members.view",
            Self::MembersManage => "members.manage",
            Self::SettingsView => "settings.view",
            Self::SettingsManage => "settings.manage",
            Self::BillingView => "billing.view",
            Self::BillingManage => "billing.manage",
            Self::TenantDelete => "tenant.delete",
            Self::OwnerAssign => "owner.assign",
            Self::PlatformTenantsList => "platform.tenants.list",
            Self::PlatformTenantsSwitch => "platform.tenants.switch",
            Self::PlatformAdmin => "platform.admin",
            Self::PlatformBillingView => "platform.billing.view",
            Self::PlatformDiagnosticsView => "platform.diagnostics.view",
        };
        f.write_str(code)
    }
}

impl FromStr for Permission {
    type Err = String;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.to_string() == code)
            .ok_or_else(|| format!("invalid permission: {code}"))
    }
}

#[cfg(test)]
mod tests {
    use super::Permission;

    #[test]
    fn permission_display_fromstr_and_serde_round_trip() {
        for permission in Permission::ALL {
            let code = permission.to_string();
            assert_eq!(code.parse::<Permission>(), Ok(permission));
            assert_eq!(
                serde_json::to_string(&permission).unwrap(),
                format!("\"{code}\"")
            );
            assert_eq!(
                serde_json::from_str::<Permission>(&format!("\"{code}\"")).unwrap(),
                permission
            );
        }
    }

    #[test]
    fn catalog_parity_with_contract() {
        let contract_codes: [&str; 25] = [
            "overview.view",
            "conversations.view",
            "conversations.manage",
            "customers.view",
            "customers.manage",
            "ai_agent.view",
            "ai_agent.manage",
            "knowledge_base.view",
            "knowledge_base.manage",
            "integrations.view",
            "integrations.manage",
            "analytics.view",
            "members.view",
            "members.manage",
            "settings.view",
            "settings.manage",
            "billing.view",
            "billing.manage",
            "tenant.delete",
            "owner.assign",
            "platform.tenants.list",
            "platform.tenants.switch",
            "platform.admin",
            "platform.billing.view",
            "platform.diagnostics.view",
        ];
        let mut implemented: Vec<String> = Permission::ALL.iter().map(|p| p.to_string()).collect();
        implemented.sort();
        let mut expected: Vec<String> = contract_codes.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(implemented, expected);
    }

    #[test]
    fn permission_rejects_unknown_codes() {
        assert!("conversations.delete".parse::<Permission>().is_err());
        assert!(serde_json::from_str::<Permission>("\"unknown\"").is_err());
    }
}
