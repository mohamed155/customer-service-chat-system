use uuid::Uuid;

use crate::model::{Classification, ToolSource};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTool {
    pub spec: ai_providers::ToolSpec,
    pub source: ToolSource,
    pub approval_required: bool,
    pub tenant_tool_id: Option<Uuid>,
}

/// Tighten-only policy resolution for a built-in tool.
///
/// Returns:
/// - `None` — tool is not enabled
/// - `Some(true)` — approval required (catalog says Approval or policy requires it)
/// - `Some(false)` — auto-approved (catalog says Auto AND policy doesn't require approval)
pub fn resolve_builtin(
    catalog_classification: Classification,
    policy_enabled: bool,
    policy_require_approval: bool,
) -> Option<bool> {
    if !policy_enabled {
        return None;
    }
    let effective =
        matches!(catalog_classification, Classification::Approval) || policy_require_approval;
    Some(effective)
}

/// A built-in tool as shown in tenant tool settings, with this tenant's
/// policy applied. Unlike [`resolve_available`] (what the engine may call),
/// this includes tools the tenant has not enabled yet — otherwise the
/// settings page could never offer them to be switched on.
#[derive(Debug, Clone)]
pub struct BuiltinSetting {
    pub spec: ai_providers::ToolSpec,
    pub classification: Classification,
    pub enabled: bool,
    pub require_approval: bool,
    pub effective_approval: bool,
}

pub async fn builtin_settings(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<BuiltinSetting>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<BuiltinSetting>> = async {
        let policies: Vec<(String, bool, bool)> = sqlx::query_as(
            "SELECT tool_name, enabled, require_approval \
         FROM tenant_tool_policies \
         WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(uow.conn())
        .await?;

        Ok(crate::registry::catalog()
            .iter()
            .map(|tool| {
                let (enabled, require_approval) = policies
                    .iter()
                    .find(|(n, _, _)| n == tool.name())
                    .map(|(_, e, r)| (*e, *r))
                    .unwrap_or((false, false));
                let classification = tool.classification();
                let effective_approval =
                    matches!(classification, Classification::Approval) || require_approval;
                BuiltinSetting {
                    spec: tool.spec(),
                    classification,
                    enabled,
                    require_approval,
                    effective_approval,
                }
            })
            .collect())
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn resolve_available(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<ResolvedTool>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<ResolvedTool>> = async {
        let catalog = crate::registry::catalog();

        // Fetch all policies for this tenant
        let policies: Vec<(String, bool, bool)> = sqlx::query_as(
            "SELECT tool_name, enabled, require_approval \
         FROM tenant_tool_policies \
         WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(uow.conn())
        .await?;

        let mut resolved: Vec<ResolvedTool> = Vec::new();

        // 1. Resolve built-in tools as before
        for tool in &catalog {
            let name = tool.name().to_string();
            let policy = policies.iter().find(|(n, _, _)| n == &name);
            let (policy_enabled, policy_require_approval) = match policy {
                Some((_, enabled, require_approval)) => (*enabled, *require_approval),
                None => continue,
            };

            if let Some(approval_required) = resolve_builtin(
                tool.classification(),
                policy_enabled,
                policy_require_approval,
            ) {
                resolved.push(ResolvedTool {
                    spec: tool.spec(),
                    source: ToolSource::Builtin,
                    approval_required,
                    tenant_tool_id: None,
                });
            }
        }

        // 2. Fetch live, enabled tenant_tools for the tenant
        let tenant_tools = crate::queries::list_tenant_tools_on(uow.conn(), tenant_id).await?;
        for tt in &tenant_tools {
            if !tt.enabled {
                continue;
            }
            let spec = ai_providers::ToolSpec {
                name: tt.name.clone(),
                description: tt.description.clone(),
                input_schema: tt.input_schema.clone(),
            };
            let approval_required = tt.classification == "approval";
            resolved.push(ResolvedTool {
                spec,
                source: ToolSource::Tenant,
                approval_required,
                tenant_tool_id: Some(tt.id),
            });
        }

        // 3. Sort by (source, name) for deterministic prompt assembly
        resolved.sort_by(|a, b| {
            let source_a = a.source.as_str();
            let source_b = b.source.as_str();
            source_a
                .cmp(source_b)
                .then_with(|| a.spec.name.cmp(&b.spec.name))
        });

        Ok(resolved)
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

/// Retained for backward compatibility — delegates to [`resolve_available`].
pub async fn resolve_available_builtins(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<ResolvedTool>> {
    resolve_available(pool, tenant_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tighten_only_auto_no_policy_require() {
        // Auto classification + policy doesn't require approval => auto
        let result = resolve_builtin(Classification::Auto, true, false);
        assert_eq!(result, Some(false));
    }

    #[test]
    fn tighten_only_auto_with_policy_require() {
        // Auto classification + policy requires approval => approval
        let result = resolve_builtin(Classification::Auto, true, true);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn tighten_only_approval_no_policy_require() {
        // Approval classification + policy doesn't require => still approval (tighten-only)
        let result = resolve_builtin(Classification::Approval, true, false);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn tighten_only_approval_with_policy_require() {
        // Both say approval => approval
        let result = resolve_builtin(Classification::Approval, true, true);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn disabled_tool_returns_none() {
        let result = resolve_builtin(Classification::Auto, false, false);
        assert_eq!(result, None);

        let result = resolve_builtin(Classification::Approval, false, true);
        assert_eq!(result, None);
    }
}
