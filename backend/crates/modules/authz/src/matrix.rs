use crate::{Permission, TenantRole};
use identity::PlatformRole;

const TENANT_ADMIN: &[Permission] = &[
    Permission::OverviewView,
    Permission::ConversationsView,
    Permission::ConversationsManage,
    Permission::CustomersView,
    Permission::CustomersManage,
    Permission::AiAgentView,
    Permission::AiAgentManage,
    Permission::KnowledgeBaseView,
    Permission::KnowledgeBaseManage,
    Permission::IntegrationsView,
    Permission::IntegrationsManage,
    Permission::AnalyticsView,
    Permission::MembersView,
    Permission::MembersManage,
    Permission::SettingsView,
    Permission::SettingsManage,
];
const TENANT_MANAGER: &[Permission] = &[
    Permission::OverviewView,
    Permission::ConversationsView,
    Permission::ConversationsManage,
    Permission::CustomersView,
    Permission::CustomersManage,
    Permission::AiAgentView,
    Permission::AiAgentManage,
    Permission::KnowledgeBaseView,
    Permission::KnowledgeBaseManage,
    Permission::IntegrationsView,
    Permission::IntegrationsManage,
    Permission::AnalyticsView,
    Permission::MembersView,
    Permission::MembersManage,
];
const TENANT_AGENT: &[Permission] = &[
    Permission::OverviewView,
    Permission::ConversationsView,
    Permission::ConversationsManage,
    Permission::CustomersView,
    Permission::CustomersManage,
    Permission::KnowledgeBaseView,
];
const TENANT_VIEWER: &[Permission] = &[
    Permission::OverviewView,
    Permission::ConversationsView,
    Permission::CustomersView,
    Permission::AiAgentView,
    Permission::KnowledgeBaseView,
    Permission::IntegrationsView,
    Permission::AnalyticsView,
];

const PLATFORM_ALL: &[Permission] = &[
    Permission::PlatformTenantsList,
    Permission::PlatformTenantsSwitch,
    Permission::PlatformAdmin,
    Permission::PlatformBillingView,
    Permission::PlatformDiagnosticsView,
];
const PLATFORM_DEVELOPER: &[Permission] = &[
    Permission::PlatformTenantsList,
    Permission::PlatformTenantsSwitch,
    Permission::PlatformDiagnosticsView,
];
const PLATFORM_TENANT_ACCESS: &[Permission] = &[
    Permission::PlatformTenantsList,
    Permission::PlatformTenantsSwitch,
];
const PLATFORM_FINANCE: &[Permission] = &[
    Permission::PlatformTenantsList,
    Permission::PlatformTenantsSwitch,
    Permission::PlatformBillingView,
];

const STAFF_PRODUCTION_DEVELOPER: &[Permission] = &[
    Permission::OverviewView,
    Permission::ConversationsView,
    Permission::CustomersView,
    Permission::AiAgentView,
    Permission::KnowledgeBaseView,
    Permission::IntegrationsView,
    Permission::AnalyticsView,
    Permission::MembersView,
    Permission::SettingsView,
];
const STAFF_PRODUCTION_SUPPORT: &[Permission] = &[
    Permission::OverviewView,
    Permission::ConversationsView,
    Permission::ConversationsManage,
    Permission::CustomersView,
    Permission::CustomersManage,
    Permission::KnowledgeBaseView,
];
const STAFF_PRODUCTION_SALES: &[Permission] = &[
    Permission::OverviewView,
    Permission::AnalyticsView,
    Permission::MembersView,
    Permission::SettingsView,
];
const STAFF_PRODUCTION_FINANCE: &[Permission] = &[
    Permission::OverviewView,
    Permission::AnalyticsView,
    Permission::MembersView,
    Permission::SettingsView,
    Permission::BillingView,
];

pub fn tenant_role_permissions(role: TenantRole) -> &'static [Permission] {
    match role {
        TenantRole::Owner => &Permission::TENANT,
        TenantRole::Admin => TENANT_ADMIN,
        TenantRole::Manager => TENANT_MANAGER,
        TenantRole::Agent => TENANT_AGENT,
        TenantRole::Viewer => TENANT_VIEWER,
    }
}

pub fn platform_role_permissions(role: PlatformRole) -> &'static [Permission] {
    match role {
        PlatformRole::SuperAdmin => PLATFORM_ALL,
        PlatformRole::Developer => PLATFORM_DEVELOPER,
        PlatformRole::Support | PlatformRole::Sales => PLATFORM_TENANT_ACCESS,
        PlatformRole::Finance => PLATFORM_FINANCE,
    }
}

pub fn staff_tenant_permissions(role: PlatformRole, is_production: bool) -> &'static [Permission] {
    if !is_production {
        return &Permission::TENANT;
    }

    match role {
        PlatformRole::SuperAdmin => &Permission::TENANT,
        PlatformRole::Developer => STAFF_PRODUCTION_DEVELOPER,
        PlatformRole::Support => STAFF_PRODUCTION_SUPPORT,
        PlatformRole::Sales => STAFF_PRODUCTION_SALES,
        PlatformRole::Finance => STAFF_PRODUCTION_FINANCE,
    }
}

#[cfg(test)]
mod tests {
    use super::{platform_role_permissions, staff_tenant_permissions, tenant_role_permissions};
    use crate::{Permission, TenantRole};
    use identity::PlatformRole;
    use std::collections::HashSet;

    fn set(permissions: &'static [Permission]) -> HashSet<Permission> {
        permissions.iter().copied().collect()
    }

    #[test]
    fn tenant_role_hierarchy_and_owner_exclusives_hold() {
        let owner = set(tenant_role_permissions(TenantRole::Owner));
        let admin = set(tenant_role_permissions(TenantRole::Admin));
        let manager = set(tenant_role_permissions(TenantRole::Manager));

        assert!(owner.is_superset(&admin));
        assert!(admin.is_superset(&manager));
        assert_eq!(
            owner.difference(&admin).copied().collect::<HashSet<_>>(),
            HashSet::from([
                Permission::BillingView,
                Permission::BillingManage,
                Permission::TenantDelete,
                Permission::OwnerAssign,
            ])
        );
    }

    #[test]
    fn manager_has_no_settings_or_billing_permissions() {
        assert!(
            tenant_role_permissions(TenantRole::Manager)
                .iter()
                .all(|permission| {
                    let code = permission.to_string();
                    !code.starts_with("settings.") && !code.starts_with("billing.")
                })
        );
    }

    #[test]
    fn viewer_has_only_view_permissions() {
        assert!(
            tenant_role_permissions(TenantRole::Viewer)
                .iter()
                .all(|permission| permission.to_string().ends_with(".view"))
        );
    }

    #[test]
    fn non_production_staff_and_production_super_admin_have_full_tenant_access() {
        let platform_roles = [
            PlatformRole::SuperAdmin,
            PlatformRole::Developer,
            PlatformRole::Support,
            PlatformRole::Sales,
            PlatformRole::Finance,
        ];
        let full_tenant_set = HashSet::from(Permission::TENANT);

        for role in platform_roles {
            assert_eq!(set(staff_tenant_permissions(role, false)), full_tenant_set);
        }
        assert_eq!(
            set(staff_tenant_permissions(PlatformRole::SuperAdmin, true)),
            full_tenant_set
        );
    }

    #[test]
    fn every_catalog_permission_is_granted_to_at_least_one_role() {
        let tenant_roles = [
            TenantRole::Owner,
            TenantRole::Admin,
            TenantRole::Manager,
            TenantRole::Agent,
            TenantRole::Viewer,
        ];
        let platform_roles = [
            PlatformRole::SuperAdmin,
            PlatformRole::Developer,
            PlatformRole::Support,
            PlatformRole::Sales,
            PlatformRole::Finance,
        ];
        let mut granted = HashSet::new();

        for role in tenant_roles {
            granted.extend(tenant_role_permissions(role));
        }
        for role in platform_roles {
            granted.extend(platform_role_permissions(role));
        }

        assert_eq!(granted, HashSet::from(Permission::ALL));
    }
}
