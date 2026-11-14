import { marker } from './marker';
import { ownValue } from './own-value';

/** Catalog keys (shared catalog) of the audit actions the API records, by action code. */
const ACTION_KEYS: Readonly<Record<string, string>> = {
  'agent_config.created': marker('audit.actions.agent_config.created'),
  'agent_config.updated': marker('audit.actions.agent_config.updated'),
  'agent_config.avatar_updated': marker('audit.actions.agent_config.avatar_updated'),
  'agent_prompt.version_created': marker('audit.actions.agent_prompt.version_created'),
  'agent_prompt.version_restored': marker('audit.actions.agent_prompt.version_restored'),
  'ai_config.updated': marker('audit.actions.ai_config.updated'),
  'ai_config.deleted': marker('audit.actions.ai_config.deleted'),
  'ai_config.capture_content_changed': marker('audit.actions.ai_config.capture_content_changed'),
  'ai_credential.set': marker('audit.actions.ai_credential.set'),
  'ai_credential.deleted': marker('audit.actions.ai_credential.deleted'),
  'auth.logged_out': marker('audit.actions.auth.logged_out'),
  'auth.login_failed': marker('audit.actions.auth.login_failed'),
  'auth.login_succeeded': marker('audit.actions.auth.login_succeeded'),
  'auth.mfa_challenged': marker('audit.actions.auth.mfa_challenged'),
  'auth.mfa_enrolled': marker('audit.actions.auth.mfa_enrolled'),
  'auth.mfa_failed': marker('audit.actions.auth.mfa_failed'),
  'auth.mfa_locked': marker('audit.actions.auth.mfa_locked'),
  'auth.mfa_recovery_codes_regenerated': marker(
    'audit.actions.auth.mfa_recovery_codes_regenerated',
  ),
  'auth.mfa_recovery_used': marker('audit.actions.auth.mfa_recovery_used'),
  'auth.mfa_reset': marker('audit.actions.auth.mfa_reset'),
  'auth.mfa_verified': marker('audit.actions.auth.mfa_verified'),
  'availability.changed': marker('audit.actions.availability.changed'),
  'billing_plan.created': marker('audit.actions.billing_plan.created'),
  'billing_plan.status_changed': marker('audit.actions.billing_plan.status_changed'),
  'billing_subscription.changed': marker('audit.actions.billing_subscription.changed'),
  'conversation.ai_handling_set': marker('audit.actions.conversation.ai_handling_set'),
  'conversation.assigned': marker('audit.actions.conversation.assigned'),
  'conversation.assignment_changed': marker('audit.actions.conversation.assignment_changed'),
  'conversation.created': marker('audit.actions.conversation.created'),
  'conversation.customer_message': marker('audit.actions.conversation.customer_message'),
  'conversation.message_status': marker('audit.actions.conversation.message_status'),
  'conversation.status_changed': marker('audit.actions.conversation.status_changed'),
  'conversation.updated': marker('audit.actions.conversation.updated'),
  'customer.created': marker('audit.actions.customer.created'),
  'customer.updated': marker('audit.actions.customer.updated'),
  'escalation.assigned': marker('audit.actions.escalation.assigned'),
  'escalation.claimed': marker('audit.actions.escalation.claimed'),
  'escalation.closed': marker('audit.actions.escalation.closed'),
  'escalation.created': marker('audit.actions.escalation.created'),
  'escalation.new': marker('audit.actions.escalation.new'),
  'escalation.queued': marker('audit.actions.escalation.queued'),
  'escalation.removed': marker('audit.actions.escalation.removed'),
  'integration.config_updated': marker('audit.actions.integration.config_updated'),
  'integration.connected': marker('audit.actions.integration.connected'),
  'integration.disconnected': marker('audit.actions.integration.disconnected'),
  'integration.secret_rotated': marker('audit.actions.integration.secret_rotated'),
  'knowledge_category.created': marker('audit.actions.knowledge_category.created'),
  'knowledge_category.deleted': marker('audit.actions.knowledge_category.deleted'),
  'knowledge_category.renamed': marker('audit.actions.knowledge_category.renamed'),
  'knowledge_document.uploaded': marker('audit.actions.knowledge_document.uploaded'),
  'knowledge_item.archived': marker('audit.actions.knowledge_item.archived'),
  'knowledge_item.created': marker('audit.actions.knowledge_item.created'),
  'knowledge_item.published': marker('audit.actions.knowledge_item.published'),
  'knowledge_item.reindex_requested': marker('audit.actions.knowledge_item.reindex_requested'),
  'knowledge_item.reindex_skipped': marker('audit.actions.knowledge_item.reindex_skipped'),
  'knowledge_item.restored': marker('audit.actions.knowledge_item.restored'),
  'knowledge_item.updated': marker('audit.actions.knowledge_item.updated'),
  'member.disabled': marker('audit.actions.member.disabled'),
  'member.enabled': marker('audit.actions.member.enabled'),
  'member.invitation_accepted': marker('audit.actions.member.invitation_accepted'),
  'member.invitation_revoked': marker('audit.actions.member.invitation_revoked'),
  'member.invited': marker('audit.actions.member.invited'),
  'member.role_changed': marker('audit.actions.member.role_changed'),
  'member.skills_changed': marker('audit.actions.member.skills_changed'),
  'platform.tenant_created': marker('audit.actions.platform.tenant_created'),
  'platform.tenant_status_changed': marker('audit.actions.platform.tenant_status_changed'),
  'platform.tenant_switched': marker('audit.actions.platform.tenant_switched'),
  'platform.tenant_updated': marker('audit.actions.platform.tenant_updated'),
  'skill.created': marker('audit.actions.skill.created'),
  'skill.deleted': marker('audit.actions.skill.deleted'),
  'skill.updated': marker('audit.actions.skill.updated'),
  'tenant.access_denied': marker('audit.actions.tenant.access_denied'),
  'tenant.signup_completed': marker('audit.actions.tenant.signup_completed'),
  'tool.approval_required': marker('audit.actions.tool.approval_required'),
  'tool.created': marker('audit.actions.tool.created'),
  'tool.deleted': marker('audit.actions.tool.deleted'),
  'tool.executed': marker('audit.actions.tool.executed'),
  'tool.policy.updated': marker('audit.actions.tool.policy.updated'),
  'tool.request.created': marker('audit.actions.tool.request.created'),
  'tool.request.updated': marker('audit.actions.tool.request.updated'),
  'tool.updated': marker('audit.actions.tool.updated'),
  'user.soft_deleted': marker('audit.actions.user.soft_deleted'),
  'widget_instance.created': marker('audit.actions.widget_instance.created'),
  'widget_instance.deleted': marker('audit.actions.widget_instance.deleted'),
  'widget_instance.updated': marker('audit.actions.widget_instance.updated'),
};

/** Catalog keys (shared catalog) of the audit categories the API groups actions into. */
const CATEGORY_KEYS: Readonly<Record<string, string>> = {
  all: marker('audit.categories.all'),
  auth: marker('audit.categories.auth'),
  tenant: marker('audit.categories.tenant'),
  members: marker('audit.categories.members'),
  prompts: marker('audit.categories.prompts'),
  ai: marker('audit.categories.ai'),
  tools: marker('audit.categories.tools'),
  billing: marker('audit.categories.billing'),
  conversations: marker('audit.categories.conversations'),
  customers: marker('audit.categories.customers'),
  escalations: marker('audit.categories.escalations'),
  knowledge: marker('audit.categories.knowledge'),
  widgets: marker('audit.categories.widgets'),
  integrations: marker('audit.categories.integrations'),
  other: marker('audit.categories.other'),
};

/** The key of an audit action's name, or `null` for an action this app has no name for. */
export const auditActionKey = (action: string): string | null =>
  ownValue(ACTION_KEYS, action) ?? null;

/** The key of an audit category's name, or `null` for one this app has no name for. */
export const auditCategoryKey = (category: string): string | null =>
  ownValue(CATEGORY_KEYS, category) ?? null;

/** The categories an audit screen can filter by, with the key of each one's name. */
export const AUDIT_FILTER_CATEGORIES: readonly { value: string; key: string }[] = [
  { value: 'all', key: marker('audit.categories.all') },
  { value: 'auth', key: marker('audit.categories.auth') },
  { value: 'tenant', key: marker('audit.categories.tenant') },
  { value: 'members', key: marker('audit.categories.members') },
  { value: 'prompts', key: marker('audit.categories.prompts') },
  { value: 'ai', key: marker('audit.categories.ai') },
  { value: 'tools', key: marker('audit.categories.tools') },
  { value: 'billing', key: marker('audit.categories.billing') },
  { value: 'conversations', key: marker('audit.categories.conversations') },
  { value: 'customers', key: marker('audit.categories.customers') },
  { value: 'escalations', key: marker('audit.categories.escalations') },
  { value: 'knowledge', key: marker('audit.categories.knowledge') },
  { value: 'widgets', key: marker('audit.categories.widgets') },
  { value: 'integrations', key: marker('audit.categories.integrations') },
];

/**
 * An audit action's name through `translate` (a function of the active language); an action this
 * app has no name for shows its code.
 */
export function auditActionName(action: string, translate: (key: string) => string): string {
  const key = auditActionKey(action);
  return key ? translate(key) : action;
}

/** An audit category's name through `translate`; a category this app has no name for shows as sent. */
export function auditCategoryName(category: string, translate: (key: string) => string): string {
  const key = auditCategoryKey(category);
  return key ? translate(key) : category;
}
