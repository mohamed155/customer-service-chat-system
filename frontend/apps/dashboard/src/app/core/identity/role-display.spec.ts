import { describe, it, expect } from 'vitest';
import { roleLabel, TENANT_ROLE_LABELS, PLATFORM_ROLE_LABELS } from './role-display';
import { MeResponse, TenantSummary } from '../api/tenant-api.models';
import { MembershipRole, PlatformRole } from '../api/tenant-api.models';

function makeUser(overrides: Partial<MeResponse> = {}): MeResponse {
  return {
    id: 'u1',
    email: 'user@test.com',
    displayName: 'Test User',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [],
    ...overrides,
  };
}

function makeTenant(overrides: Partial<TenantSummary> = {}): TenantSummary {
  return {
    id: 't1',
    name: 'Acme Corp',
    slug: 'acme',
    status: 'active',
    ...overrides,
  };
}

describe('roleLabel', () => {
  describe('null/empty cases', () => {
    it('returns null when user is null', () => {
      expect(roleLabel(null, makeTenant())).toBeNull();
    });

    it('returns null when user has no platform role and no active tenant', () => {
      expect(roleLabel(makeUser(), null)).toBeNull();
    });

    it('returns null when user has no platform role and active tenant is null', () => {
      expect(roleLabel(makeUser(), null)).toBeNull();
    });

    it('returns null when user has no memberships and active tenant is provided', () => {
      const user = makeUser({ memberships: [] });
      expect(roleLabel(user, makeTenant())).toBeNull();
    });

    it('returns null when membership for active tenant does not exist', () => {
      const user = makeUser({
        memberships: [
          {
            tenantId: 't2',
            tenantName: 'Other Corp',
            tenantSlug: 'other',
            role: 'viewer' as MembershipRole,
            permissions: [],
          },
        ],
      });
      expect(roleLabel(user, makeTenant({ id: 't1', name: 'Acme Corp', slug: 'acme', status: 'active' }))).toBeNull();
    });

    it('returns null when user has no platform role and no memberships', () => {
      const user = makeUser({ platformRole: null, memberships: [] });
      expect(roleLabel(user, makeTenant())).toBeNull();
    });
  });

  describe('platform roles', () => {
    const platformRoles: Array<{ key: PlatformRole; expected: string }> = [
      { key: 'super_admin', expected: 'Super Admin' },
      { key: 'developer', expected: 'Developer' },
      { key: 'support', expected: 'Support Engineer' },
      { key: 'sales', expected: 'Sales' },
      { key: 'finance', expected: 'Finance' },
    ];

    platformRoles.forEach(({ key, expected }) => {
      it(`returns "${expected}" for platform role "${key}"`, () => {
        const user = makeUser({ platformRole: key });
        expect(roleLabel(user, null)).toBe(expected);
      });
    });

    it('returns platform role label even when active tenant is present (staff takes priority)', () => {
      const user = makeUser({
        platformRole: 'super_admin',
        memberships: [
          {
            tenantId: 't1',
            tenantName: 'Acme Corp',
            tenantSlug: 'acme',
            role: 'admin' as MembershipRole,
            permissions: [],
          },
        ],
      });
      expect(roleLabel(user, makeTenant())).toBe('Super Admin');
    });
  });

  describe('tenant roles', () => {
    const tenantRoles: Array<{ key: MembershipRole; expected: string }> = [
      { key: 'owner', expected: 'Owner' },
      { key: 'admin', expected: 'Admin' },
      { key: 'manager', expected: 'Manager' },
      { key: 'agent', expected: 'Support Agent' },
      { key: 'viewer', expected: 'Viewer' },
    ];

    tenantRoles.forEach(({ key, expected }) => {
      it(`returns "${expected} · Acme Corp" for tenant role "${key}"`, () => {
        const user = makeUser({
          platformRole: null,
          memberships: [
            {
              tenantId: 't1',
              tenantName: 'Acme Corp',
              tenantSlug: 'acme',
              role: key,
              permissions: [],
            },
          ],
        });
        const tenant = makeTenant({ id: 't1', name: 'Acme Corp' });
        expect(roleLabel(user, tenant)).toBe(`${expected} · Acme Corp`);
      });
    });

    it('returns label with tenant name from activeTenant parameter', () => {
      const user = makeUser({
        platformRole: null,
        memberships: [
          {
            tenantId: 't1',
            tenantName: 'Acme Corp',
            tenantSlug: 'acme',
            role: 'owner' as MembershipRole,
            permissions: [],
          },
        ],
      });
      const tenant = makeTenant({ id: 't1', name: 'Widgets Inc' });
      expect(roleLabel(user, tenant)).toBe('Owner · Widgets Inc');
    });
  });

  describe('label maps export correctness', () => {
    it('has all 5 tenant role labels', () => {
      expect(TENANT_ROLE_LABELS).toEqual({
        owner: 'Owner',
        admin: 'Admin',
        manager: 'Manager',
        agent: 'Support Agent',
        viewer: 'Viewer',
      });
    });

    it('has all 5 platform role labels', () => {
      expect(PLATFORM_ROLE_LABELS).toEqual({
        super_admin: 'Super Admin',
        developer: 'Developer',
        support: 'Support Engineer',
        sales: 'Sales',
        finance: 'Finance',
      });
    });
  });
});
