import type { components } from './generated/api';
import { membershipFromWire, meFromWire, tenantSummaryFromWire } from './tenant-api.models';

type Schemas = components['schemas'];

// `/me` and the workspace summaries arrive typed as the generated schemas (roles, permissions,
// status and plan are plain strings there). The mappers give the dashboard its narrower types.
describe('wire mappers for the session types (MOH-169)', () => {
  const membership: Schemas['MembershipSummary'] = {
    tenantId: '0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f',
    tenantName: 'Acme Support',
    tenantSlug: 'acme-support',
    role: 'owner',
    permissions: ['overview.view', 'customers.view'],
  };

  it('maps a membership field by field', () => {
    expect(membershipFromWire(membership)).toEqual(membership);
  });

  it('maps a tenant user: no platform role, memberships mapped', () => {
    const wire: Schemas['MeResponse'] = {
      id: '0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e00',
      email: 'owner@acme.test',
      displayName: 'Olivia Owner',
      platformPermissions: [],
      memberships: [membership],
    };

    const me = meFromWire(wire);

    expect(me).toEqual({
      id: wire.id,
      email: 'owner@acme.test',
      displayName: 'Olivia Owner',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [membership],
    });
  });

  it('maps a platform user: role and staff permissions kept, null where the wire says null', () => {
    const wire: Schemas['MeResponse'] = {
      id: '0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e01',
      email: 'admin@helix.test',
      displayName: 'Priya Platform',
      platformRole: 'super_admin',
      platformPermissions: ['platform.admin', 'platform.tenants.list'],
      staffTenantPermissions: ['overview.view'],
      memberships: [],
    };

    const me = meFromWire(wire);

    expect(me.platformRole).toBe('super_admin');
    expect(me.platformPermissions).toEqual(['platform.admin', 'platform.tenants.list']);
    expect(me.staffTenantPermissions).toEqual(['overview.view']);
    expect(meFromWire({ ...wire, platformRole: null, staffTenantPermissions: null })).toMatchObject(
      {
        platformRole: null,
        staffTenantPermissions: null,
      },
    );
  });

  it('does not carry a field the dashboard does not know', () => {
    const wire = {
      id: 'x',
      email: 'a@b.test',
      displayName: 'A',
      platformPermissions: [],
      memberships: [],
      somethingNew: 'secret',
    } as Schemas['MeResponse'];

    expect(Object.keys(meFromWire(wire)).sort()).toEqual([
      'displayName',
      'email',
      'id',
      'memberships',
      'platformPermissions',
      'platformRole',
      'staffTenantPermissions',
    ]);
  });

  it('maps a tenant summary', () => {
    const wire: Schemas['TenantSummary'] = {
      id: '0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f',
      name: 'Acme Support',
      slug: 'acme-support',
      status: 'active',
      plan: 'starter',
    };

    expect(tenantSummaryFromWire(wire)).toEqual(wire);
  });
});
