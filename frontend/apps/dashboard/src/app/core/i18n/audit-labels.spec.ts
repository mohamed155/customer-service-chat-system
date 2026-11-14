import {
  AUDIT_FILTER_CATEGORIES,
  auditActionKey,
  auditActionName,
  auditCategoryKey,
  auditCategoryName,
} from './audit-labels';
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

  it('writes a name through the translate function, and the code of one it has no name for', () => {
    const translate = (key: string) => `[${key}]`;
    expect(auditActionName('member.disabled', translate)).toBe('[audit.actions.member.disabled]');
    expect(auditActionName('something.brand_new', translate)).toBe('something.brand_new');
    expect(auditCategoryName('members', translate)).toBe('[audit.categories.members]');
    expect(auditCategoryName('elsewhere', translate)).toBe('elsewhere');
  });

  it('filters by the integrations category the API takes, and not by "other", which it refuses', () => {
    const values = AUDIT_FILTER_CATEGORIES.map((c) => c.value);
    expect(values).toContain('integrations');
    expect(values).not.toContain('other');
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
