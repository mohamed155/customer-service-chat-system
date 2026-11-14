import { AUDIT_FILTER_CATEGORIES, auditActionKey, auditCategoryKey } from './audit-labels';
import sharedEnglish from '../../../../public/i18n/shared/en.json';

const lookup = (key: string): unknown =>
  key
    .split('.')
    .reduce<unknown>((node, part) => (node as Record<string, unknown>)?.[part], sharedEnglish);

describe('audit labels', () => {
  it('gives the key of an action and of a category the app has names for', () => {
    expect(auditActionKey('member.disabled')).toBe('audit.actions.member.disabled');
    expect(auditActionKey('tool.policy.updated')).toBe('audit.actions.tool.policy.updated');
    expect(auditCategoryKey('members')).toBe('audit.categories.members');
  });

  it('gives none for an action or category it has no name for, and never reads a prototype property', () => {
    expect(auditActionKey('something.brand_new')).toBeNull();
    expect(auditActionKey('constructor')).toBeNull();
    expect(auditCategoryKey('__proto__')).toBeNull();
  });

  it('has a name in the shared catalog for every key it hands out', () => {
    for (const { key } of AUDIT_FILTER_CATEGORIES) {
      expect(typeof lookup(key), key).toBe('string');
    }
    for (const action of ['agent_config.created', 'auth.mfa_reset', 'widget_instance.deleted']) {
      expect(typeof lookup(auditActionKey(action)!), action).toBe('string');
    }
  });
});
