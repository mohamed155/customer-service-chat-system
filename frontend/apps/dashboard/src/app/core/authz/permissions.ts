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
  | 'platform.admin'
  | 'platform.billing.view'
  | 'platform.diagnostics.view';

export const PAGE_PERMISSIONS = {
  [APP_PATHS.tenant.overview]: 'overview.view',
  [APP_PATHS.tenant.conversations]: 'conversations.view',
  [APP_PATHS.tenant.customers]: 'customers.view',
  [APP_PATHS.tenant.aiAgent]: 'ai_agent.view',
  [APP_PATHS.tenant.knowledgeBase]: 'knowledge_base.view',
  [APP_PATHS.tenant.integrations]: 'integrations.view',
  [APP_PATHS.tenant.analytics]: 'analytics.view',
  [APP_PATHS.tenant.settings]: 'settings.view',
  [APP_PATHS.platform.base]: 'platform.admin',
} as const satisfies Record<string, Permission>;
