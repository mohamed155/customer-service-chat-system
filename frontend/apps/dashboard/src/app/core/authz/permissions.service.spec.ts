import { TestBed } from '@angular/core/testing';
import { MeResponse, TenantSummary } from '../api/tenant-api.models';
import { CurrentUserService } from '../tenant/current-user.service';
import { TenantContextService } from '../tenant/tenant-context.service';
import { PermissionsService } from './permissions.service';

describe('PermissionsService', () => {
  let service: PermissionsService;
  let currentUser: { currentUser: ReturnType<typeof vi.fn> };
  let tenantContext: { activeTenant: ReturnType<typeof vi.fn> };

  const configure = (user: MeResponse | null, activeTenantId: string | null) => {
    currentUser = { currentUser: vi.fn(() => user) };
    const activeTenant: TenantSummary | null = activeTenantId
      ? {
          id: activeTenantId,
          name: 'T1',
          slug: 't1',
          status: 'active' as const,
          plan: 'trial' as const,
        }
      : null;
    tenantContext = { activeTenant: vi.fn(() => activeTenant) };
    TestBed.configureTestingModule({
      providers: [
        PermissionsService,
        { provide: CurrentUserService, useValue: currentUser },
        { provide: TenantContextService, useValue: tenantContext },
      ],
    });
    service = TestBed.inject(PermissionsService);
  };

  it('returns empty set when no user is loaded', () => {
    configure(null, null);
    expect(service.effective().size).toBe(0);
    expect(service.has('overview.view')).toBe(false);
  });

  it('returns tenant user permissions matching the active tenant', () => {
    const user: MeResponse = {
      id: 'u-1',
      email: 'agent@test.com',
      displayName: 'Agent',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [
        {
          tenantId: 't1',
          tenantName: 'T1',
          tenantSlug: 't1',
          role: 'agent',
          permissions: [
            'overview.view',
            'conversations.view',
            'conversations.manage',
            'customers.view',
            'customers.manage',
            'knowledge_base.view',
          ],
        },
        {
          tenantId: 't2',
          tenantName: 'T2',
          tenantSlug: 't2',
          role: 'viewer',
          permissions: [
            'overview.view',
            'conversations.view',
            'customers.view',
            'ai_agent.view',
            'knowledge_base.view',
            'integrations.view',
            'analytics.view',
          ],
        },
      ],
    };
    configure(user, 't2');

    expect(service.has('overview.view')).toBe(true);
    expect(service.has('conversations.view')).toBe(true);
    expect(service.has('conversations.manage')).toBe(false); // viewer has no manage
    expect(service.has('settings.view')).toBe(false);
    expect(service.has('platform.tenants.list')).toBe(false);
  });

  it('returns platform user platformPermissions without active tenant', () => {
    const user: MeResponse = {
      id: 'u-1',
      email: 'admin@test.com',
      displayName: 'Admin',
      platformRole: 'super_admin',
      platformPermissions: [
        'platform.tenants.list',
        'platform.tenants.switch',
        'platform.admin',
        'platform.billing.view',
        'platform.diagnostics.view',
      ],
      staffTenantPermissions: [
        'overview.view',
        'conversations.view',
        'conversations.manage',
        'customers.view',
        'customers.manage',
        'ai_agent.view',
        'ai_agent.manage',
        'knowledge_base.view',
        'knowledge_base.manage',
        'integrations.view',
        'integrations.manage',
        'analytics.view',
        'members.view',
        'members.manage',
        'settings.view',
        'settings.manage',
        'billing.view',
        'billing.manage',
        'tenant.delete',
        'owner.assign',
      ],
      memberships: [],
    };
    configure(user, null);

    expect(service.has('platform.admin')).toBe(true);
    expect(service.has('platform.tenants.list')).toBe(true);
    expect(service.has('overview.view')).toBe(false); // no active tenant → only platform perms
  });

  it('returns platform user staffTenantPermissions with active tenant', () => {
    const user: MeResponse = {
      id: 'u-1',
      email: 'support@test.com',
      displayName: 'Support',
      platformRole: 'support',
      platformPermissions: ['platform.tenants.list', 'platform.tenants.switch'],
      staffTenantPermissions: [
        'overview.view',
        'conversations.view',
        'customers.view',
        'knowledge_base.view',
      ],
      memberships: [],
    };
    configure(user, 't1');

    expect(service.has('overview.view')).toBe(true);
    expect(service.has('conversations.view')).toBe(true);
    expect(service.has('conversations.manage')).toBe(false);
    expect(service.has('settings.view')).toBe(false);
    expect(service.has('platform.tenants.list')).toBe(true);
  });

  it('uses staffTenantPermissions (not memberships) for platform user with active tenant', () => {
    const user: MeResponse = {
      id: 'u-1',
      email: 'support@test.com',
      displayName: 'Support Engineer',
      platformRole: 'support',
      platformPermissions: ['platform.tenants.list', 'platform.tenants.switch'],
      staffTenantPermissions: [
        'overview.view',
        'conversations.view',
        'conversations.manage',
        'customers.view',
        'customers.manage',
        'knowledge_base.view',
      ],
      memberships: [],
    };
    configure(user, 't1');

    expect(service.has('conversations.view')).toBe(true);
    expect(service.has('settings.view')).toBe(false);
    expect(service.has('platform.tenants.list')).toBe(true);
  });

  it('returns empty set for tenant user with no matching membership', () => {
    const user: MeResponse = {
      id: 'u-1',
      email: 'agent@test.com',
      displayName: 'Agent',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [
        {
          tenantId: 't1',
          tenantName: 'T1',
          tenantSlug: 't1',
          role: 'agent',
          permissions: ['overview.view', 'conversations.view'],
        },
      ],
    };
    configure(user, 't2');

    expect(service.has('overview.view')).toBe(false);
    expect(service.effective().size).toBe(0);
  });
});
