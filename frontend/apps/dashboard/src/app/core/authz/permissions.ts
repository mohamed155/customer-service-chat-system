import { APP_PATHS } from '../router/app-paths';

export type Permission =
  | 'overview.view'
  | 'conversations.view'
  | 'conversations.manage'
  | 'customers.view'
  | 'customers.manage'
  | 'ai_agent.view'
  | 'ai_agent.manage'
  | 'knowledge_base.view'
  | 'knowledge_base.manage'
  | 'integrations.view'
  | 'integrations.manage'
  | 'analytics.view'
  | 'audit.view'
  | 'members.view'
  | 'members.manage'
  | 'settings.view'
  | 'settings.manage'
  | 'billing.view'
  | 'billing.manage'
  | 'tenant.delete'
  | 'owner.assign'
  | 'platform.tenants.list'
  | 'platform.tenants.switch'
  | 'platform.tenants.manage'
  | 'platform.admin'
  | 'platform.billing.view'
  | 'platform.diagnostics.view'
  | 'platform.audit.view'
  | 'widgets.view'
  | 'widgets.manage';

export const PAGE_PERMISSIONS = {
  [APP_PATHS.tenant.overview]: 'overview.view',
  [APP_PATHS.tenant.conversations]: 'conversations.view',
  [APP_PATHS.tenant.conversationDetail(':id')]: 'conversations.view',
  [APP_PATHS.tenant.customers]: 'customers.view',
  [APP_PATHS.tenant.customerDetail(':id')]: 'customers.view',
  [APP_PATHS.tenant.aiAgent]: 'ai_agent.view',
  [APP_PATHS.tenant.aiAgentPrompt]: 'ai_agent.view',
  [APP_PATHS.tenant.knowledgeBase]: 'knowledge_base.view',
  [APP_PATHS.tenant.integrations]: 'integrations.view',
  [APP_PATHS.tenant.integrationDetail]: 'integrations.view',
  [APP_PATHS.tenant.analytics]: 'analytics.view',
  [APP_PATHS.tenant.settings]: 'settings.view',
  [APP_PATHS.tenant.settingsTools]: 'settings.view',
  [APP_PATHS.tenant.team]: 'members.view',
  [APP_PATHS.platform.base]: 'platform.admin',
  [APP_PATHS.tenant.escalations]: 'conversations.view',
  [APP_PATHS.tenant.widgets]: 'widgets.view',
  [APP_PATHS.tenant.notifications]: 'overview.view',
  [APP_PATHS.tenant.auditLogs]: 'audit.view',
  [APP_PATHS.platform.auditLogs]: 'platform.audit.view',
} as const satisfies Record<string, Permission>;
