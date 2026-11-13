import { describe, it, expect } from 'vitest';
import { roleLine, TENANT_ROLE_KEYS, PLATFORM_ROLE_KEYS } from './role-display';
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
    plan: 'trial',
    ...overrides,
  };
}

describe('roleLine', () => {
  describe('null/empty cases', () => {
    it('returns null when user is null', () => {
      expect(roleLine(null, makeTenant())).toBeNull();
    });

    it('returns null when user has no platform role and no active tenant', () => {
      expect(roleLine(makeUser(), null)).toBeNull();
    });

    it('returns null when user has no platform role and active tenant is null', () => {
      expect(roleLine(makeUser(), null)).toBeNull();
    });

    it('returns null when user has no memberships and active tenant is provided', () => {
      const user = makeUser({ memberships: [] });
      expect(roleLine(user, makeTenant())).toBeNull();
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
      expect(
        roleLine(
          user,
          makeTenant({
            id: 't1',
            name: 'Acme Corp',
            slug: 'acme',
            status: 'active',
            plan: 'trial',
          }),
        ),
      ).toBeNull();
    });

    it('returns null when user has no platform role and no memberships', () => {
      const user = makeUser({ platformRole: null, memberships: [] });
      expect(roleLine(user, makeTenant())).toBeNull();
    });
  });

  describe('platform roles', () => {
    const platformRoles: Array<{ key: PlatformRole; expected: string }> = [
      { key: 'super_admin', expected: 'roles.platform.super_admin' },
      { key: 'developer', expected: 'roles.platform.developer' },
      { key: 'support', expected: 'roles.platform.support' },
      { key: 'sales', expected: 'roles.platform.sales' },
      { key: 'finance', expected: 'roles.platform.finance' },
    ];

    platformRoles.forEach(({ key, expected }) => {
      it(`returns "${expected}" for platform role "${key}"`, () => {
        const user = makeUser({ platformRole: key });
        expect(roleLine(user, null)).toEqual({ key: expected });
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
      expect(roleLine(user, makeTenant())).toEqual({ key: 'roles.platform.super_admin' });
    });
  });

  describe('tenant roles', () => {
    const tenantRoles: Array<{ key: MembershipRole; expected: string }> = [
      { key: 'owner', expected: 'roles.tenant.owner' },
      { key: 'admin', expected: 'roles.tenant.admin' },
      { key: 'manager', expected: 'roles.tenant.manager' },
      { key: 'agent', expected: 'roles.tenant.agent' },
      { key: 'viewer', expected: 'roles.tenant.viewer' },
    ];

    tenantRoles.forEach(({ key, expected }) => {
      it(`returns "${expected}" with the workspace name for tenant role "${key}"`, () => {
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
        expect(roleLine(user, tenant)).toEqual({ key: expected, params: { tenant: 'Acme Corp' } });
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
      expect(roleLine(user, tenant)).toEqual({
        key: 'roles.tenant.owner',
        params: { tenant: 'Widgets Inc' },
      });
    });
  });

  describe('roles the app does not know', () => {
    it('shows no line for a role named like a property of Object', () => {
      const user = makeUser({ platformRole: 'constructor' as PlatformRole });
      expect(roleLine(user, null)).toBeNull();
      const member = makeUser({
        memberships: [
          {
            tenantId: 't1',
            tenantName: 'Acme Corp',
            tenantSlug: 'acme',
            role: 'toString' as MembershipRole,
            permissions: [],
          },
        ],
      });
      expect(roleLine(member, makeTenant())).toBeNull();
    });
  });

  describe('key maps', () => {
    it('has all 5 tenant role keys', () => {
      expect(TENANT_ROLE_KEYS).toEqual({
        owner: 'roles.tenant.owner',
        admin: 'roles.tenant.admin',
        manager: 'roles.tenant.manager',
        agent: 'roles.tenant.agent',
        viewer: 'roles.tenant.viewer',
      });
    });

    it('has all 5 platform role keys', () => {
      expect(PLATFORM_ROLE_KEYS).toEqual({
        super_admin: 'roles.platform.super_admin',
        developer: 'roles.platform.developer',
        support: 'roles.platform.support',
        sales: 'roles.platform.sales',
        finance: 'roles.platform.finance',
      });
    });
  });
});
