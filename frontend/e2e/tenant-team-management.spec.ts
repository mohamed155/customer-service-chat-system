import { expect, Page, Route, test } from '@playwright/test';

type MemberRole = 'owner' | 'admin' | 'manager' | 'agent' | 'viewer';
type MemberStatus = 'active' | 'disabled';

type Tenant = {
  id: string;
  name: string;
  slug: string;
  status: 'active' | 'suspended';
  plan: 'trial' | 'starter' | 'professional' | 'enterprise';
};

type TeamMember = {
  id: string;
  userId: string;
  displayName: string;
  email: string;
  role: MemberRole;
  status: MemberStatus;
  joinedAt: string;
};

type Invitation = {
  id: string;
  token: string;
  tenantId: string;
  email: string;
  role: MemberRole;
  status: 'pending' | 'accepted' | 'revoked';
  invitedByName: string;
  createdAt: string;
  expiresAt: string;
};

type MembershipSummary = {
  tenantId: string;
  tenantName: string;
  tenantSlug: string;
  role: MemberRole;
  permissions: string[];
};

type CurrentUser = {
  id: string;
  email: string;
  displayName: string;
  platformRole: 'super_admin' | 'developer' | 'sales' | 'support' | 'finance' | null;
  platformPermissions: string[];
  staffTenantPermissions: string[] | null;
  memberships: MembershipSummary[];
};

type TeamState = {
  currentUser: CurrentUser | null;
  activeTenantId: string | null;
  tenants: Tenant[];
  membersByTenant: Record<string, TeamMember[]>;
  invitationsByTenant: Record<string, Invitation[]>;
  nextInvitationId: number;
  nextMemberId: number;
};

const TEAM_PERMISSIONS = ['members.view', 'members.manage', 'owner.assign'];
const VIEW_PERMISSIONS = ['members.view', 'members.manage'];

const tenantA: Tenant = {
  id: 'tenant-a',
  name: 'Acme Support',
  slug: 'acme-support',
  status: 'active',
  plan: 'starter',
};

const tenantB: Tenant = {
  id: 'tenant-b',
  name: 'Northwind Help',
  slug: 'northwind-help',
  status: 'active',
  plan: 'professional',
};

function makeMember(input: {
  id: string;
  userId: string;
  displayName: string;
  email: string;
  role: MemberRole;
  status?: MemberStatus;
  joinedAt?: string;
}): TeamMember {
  return {
    status: 'active',
    joinedAt: '2026-07-01T09:00:00Z',
    ...input,
  };
}

function makeOwnerIdentity(): CurrentUser {
  return {
    id: 'user-owner',
    email: 'owner@acme.test',
    displayName: 'Olivia Owner',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: tenantA.id,
        tenantName: tenantA.name,
        tenantSlug: tenantA.slug,
        role: 'owner',
        permissions: TEAM_PERMISSIONS,
      },
    ],
  };
}

function makeManagerIdentity(): CurrentUser {
  return {
    id: 'user-manager',
    email: 'manager@acme.test',
    displayName: 'Marcus Manager',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: tenantA.id,
        tenantName: tenantA.name,
        tenantSlug: tenantA.slug,
        role: 'manager',
        permissions: VIEW_PERMISSIONS,
      },
    ],
  };
}

function makePlatformIdentity(): CurrentUser {
  return {
    id: 'user-platform',
    email: 'superadmin@helix.test',
    displayName: 'Priya Platform',
    platformRole: 'super_admin',
    platformPermissions: ['platform.admin', 'platform.tenants.list', 'platform.tenants.switch'],
    staffTenantPermissions: TEAM_PERMISSIONS,
    memberships: [],
  };
}

function makeState(identity: CurrentUser | null, activeTenantId: string | null): TeamState {
  return {
    currentUser: identity,
    activeTenantId,
    tenants: [tenantA, tenantB],
    membersByTenant: {
      [tenantA.id]: [
        makeMember({
          id: 'member-a-owner',
          userId: 'user-owner',
          displayName: 'Olivia Owner',
          email: 'owner@acme.test',
          role: 'owner',
        }),
        makeMember({
          id: 'member-a-admin',
          userId: 'user-admin',
          displayName: 'Alice Admin',
          email: 'admin@acme.test',
          role: 'admin',
        }),
        makeMember({
          id: 'member-a-agent',
          userId: 'user-agent',
          displayName: 'Ava Agent',
          email: 'agent@acme.test',
          role: 'agent',
        }),
      ],
      [tenantB.id]: [
        makeMember({
          id: 'member-b-owner',
          userId: 'user-owner-b',
          displayName: 'Noah Owner',
          email: 'owner@northwind.test',
          role: 'owner',
        }),
        makeMember({
          id: 'member-b-manager',
          userId: 'user-manager-b',
          displayName: 'Maya Manager',
          email: 'manager@northwind.test',
          role: 'manager',
        }),
      ],
    },
    invitationsByTenant: {
      [tenantA.id]: [],
      [tenantB.id]: [],
    },
    nextInvitationId: 1,
    nextMemberId: 1,
  };
}

function json(route: Route, data: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

function apiError(code: string, message: string, status: number) {
  return {
    error: {
      code,
      message,
      details: [],
    },
    status,
  };
}

function permissionsForRole(role: MemberRole): string[] {
  if (role === 'owner' || role === 'admin' || role === 'manager') {
    return ['members.view', 'members.manage'];
  }
  return [];
}

function membershipFromTenant(tenant: Tenant, role: MemberRole): MembershipSummary {
  return {
    tenantId: tenant.id,
    tenantName: tenant.name,
    tenantSlug: tenant.slug,
    role,
    permissions: permissionsForRole(role),
  };
}

async function installTeamApi(page: Page, state: TeamState) {
  await page.context().route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    const method = route.request().method();
    const segments = path.split('/').filter(Boolean);
    const headers = route.request().headers();
    const tenantId = headers['x-tenant-id'] ?? state.activeTenantId ?? tenantA.id;
    const tenant = state.tenants.find((item) => item.id === tenantId) ?? state.tenants[0];
    const currentUser = state.currentUser;

    if (path === '/me') {
      if (!currentUser) {
        return json(route, apiError('unauthenticated', 'Authentication required', 401), 401);
      }
      return json(route, currentUser);
    }

    if (method === 'GET' && path === '/platform/tenants') {
      return json(route, { items: state.tenants, nextCursor: null, hasMore: false });
    }

    if (
      method === 'POST' &&
      segments[0] === 'platform' &&
      segments[1] === 'tenants' &&
      segments[3] === 'switch'
    ) {
      const targetTenant = state.tenants.find((item) => item.id === segments[2]);
      if (!targetTenant) {
        return json(route, apiError('not_found', 'Tenant not found', 404), 404);
      }
      state.activeTenantId = targetTenant.id;
      return json(route, targetTenant);
    }

    if (method === 'GET' && path === '/tenant/members') {
      const q = url.searchParams.get('q')?.toLowerCase();
      const status = url.searchParams.get('status');
      let items = [...(state.membersByTenant[tenant.id] ?? [])];
      if (q) {
        items = items.filter(
          (item) =>
            item.displayName.toLowerCase().includes(q) || item.email.toLowerCase().includes(q),
        );
      }
      if (status === 'active' || status === 'disabled') {
        items = items.filter((item) => item.status === status);
      }
      return json(route, { items, nextCursor: null, hasMore: false });
    }

    if (method === 'GET' && path === '/tenant/members/invitations') {
      const status = url.searchParams.get('status');
      let items = [...(state.invitationsByTenant[tenant.id] ?? [])].map((invitation) => ({
        id: invitation.id,
        email: invitation.email,
        role: invitation.role,
        status:
          invitation.status === 'pending' && new Date(invitation.expiresAt).getTime() < Date.now()
            ? 'expired'
            : invitation.status,
        invitedByName: invitation.invitedByName,
        createdAt: invitation.createdAt,
        expiresAt: invitation.expiresAt,
      }));
      if (status && status !== 'all') {
        items = items.filter((item) => item.status === status);
      }
      return json(route, { items, nextCursor: null, hasMore: false });
    }

    if (method === 'POST' && path === '/tenant/members/invitations') {
      const body = route.request().postDataJSON() as { email: string; role: MemberRole };
      const invitation: Invitation = {
        id: `inv-${state.nextInvitationId++}`,
        token: `token-${state.nextInvitationId}`,
        tenantId: tenant.id,
        email: body.email,
        role: body.role,
        status: 'pending',
        invitedByName: currentUser?.displayName ?? 'Olivia Owner',
        createdAt: '2026-07-12T09:00:00Z',
        expiresAt: '2026-07-19T09:00:00Z',
      };
      state.invitationsByTenant[tenant.id].unshift(invitation);
      return json(
        route,
        {
          invitation: {
            id: invitation.id,
            email: invitation.email,
            role: invitation.role,
            status: invitation.status,
            invitedByName: invitation.invitedByName,
            createdAt: invitation.createdAt,
            expiresAt: invitation.expiresAt,
          },
          acceptUrl: `http://127.0.0.1:4201/invite/${invitation.token}`,
          emailSent: false,
          emailDeliveryStatus: 'unconfigured',
        },
        201,
      );
    }

    if (method === 'PATCH' && path.startsWith('/tenant/members/')) {
      const memberId = path.split('/').pop() ?? '';
      const body = route.request().postDataJSON() as { role?: MemberRole; status?: MemberStatus };
      const members = state.membersByTenant[tenant.id] ?? [];
      const member = members.find((item) => item.id === memberId);

      if (!member) {
        return json(route, apiError('not_found', 'Member not found', 404), 404);
      }

      const actor = state.currentUser;
      const actorMembership = actor?.memberships.find(
        (membership) => membership.tenantId === tenant.id,
      );
      const actorRole = actorMembership?.role ?? 'owner';
      const isManager = actorRole === 'manager';
      const targetRank: Record<MemberRole, number> = {
        owner: 5,
        admin: 4,
        manager: 3,
        agent: 2,
        viewer: 1,
      };
      const actorRank = targetRank[actorRole];

      if (body.role) {
        if (isManager && targetRank[member.role] >= actorRank) {
          return json(route, apiError('forbidden', 'Access denied', 403), 403);
        }
        member.role = body.role;
        return json(route, member);
      }

      if (body.status) {
        if (isManager && targetRank[member.role] >= actorRank) {
          return json(route, apiError('forbidden', 'Access denied', 403), 403);
        }
        member.status = body.status;
        return json(route, member);
      }

      return json(route, apiError('validation_failed', 'Validation failed', 422), 422);
    }

    if (method === 'DELETE' && path.startsWith('/tenant/members/invitations/')) {
      const invitationId = path.split('/').pop() ?? '';
      const invitations = state.invitationsByTenant[tenant.id] ?? [];
      const invitation = invitations.find((item) => item.id === invitationId);
      if (!invitation) {
        return json(route, apiError('not_found', 'Invitation not found', 404), 404);
      }
      invitation.status = 'revoked';
      return json(route, null, 204);
    }

    if (method === 'GET' && path.startsWith('/invitations/')) {
      const token = path.split('/')[2];
      for (const tenantInvitations of Object.values(state.invitationsByTenant)) {
        const invitation = tenantInvitations.find((item) => item.token === token);
        if (invitation) {
          if (invitation.status !== 'pending') {
            return json(route, apiError('not_found', 'Invitation not found', 404), 404);
          }
          return json(route, {
            tenantName: tenant.name,
            email: invitation.email,
            role: invitation.role,
            expiresAt: invitation.expiresAt,
            accountExists: false,
          });
        }
      }
      return json(route, apiError('not_found', 'Invitation not found', 404), 404);
    }

    if (method === 'POST' && path.startsWith('/invitations/') && path.endsWith('/accept')) {
      const token = path.split('/')[2];
      for (const [tenantKey, tenantInvitations] of Object.entries(state.invitationsByTenant)) {
        const invitation = tenantInvitations.find((item) => item.token === token);
        if (!invitation) continue;

        const body = route.request().postDataJSON() as { displayName?: string; password?: string };
        if (
          state.currentUser &&
          state.currentUser.email.trim().toLowerCase() !== invitation.email.trim().toLowerCase()
        ) {
          return json(
            route,
            apiError('forbidden', 'This invitation was issued to a different email address.', 403),
            403,
          );
        }

        state.currentUser = {
          id: `user-accepted-${invitation.id}`,
          email: invitation.email,
          displayName: body.displayName ?? state.currentUser?.displayName ?? 'Accepted User',
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: [
            membershipFromTenant(
              state.tenants.find((item) => item.id === tenantKey) ?? tenant,
              invitation.role,
            ),
          ],
        };
        const tenantMembers = state.membersByTenant[tenantKey] ?? [];
        if (
          !tenantMembers.some(
            (member) => member.email.trim().toLowerCase() === invitation.email.trim().toLowerCase(),
          )
        ) {
          tenantMembers.unshift({
            id: `member-accepted-${invitation.id}`,
            userId: state.currentUser.id,
            displayName: state.currentUser.displayName,
            email: invitation.email,
            role: invitation.role,
            status: 'active',
            joinedAt: invitation.createdAt,
          });
          state.membersByTenant[tenantKey] = tenantMembers;
        }
        invitation.status = 'accepted';
        state.activeTenantId = tenantKey;
        return json(route, {
          id: state.currentUser.id,
          email: state.currentUser.email,
          displayName: state.currentUser.displayName,
          platformRole: null,
          platformPermissions: [],
          staffTenantPermissions: null,
          memberships: state.currentUser.memberships,
        });
      }

      return json(route, apiError('not_found', 'Invitation not found', 404), 404);
    }

    return json(route, { items: [], nextCursor: null, hasMore: false });
  });
}

test.describe('tenant team management', () => {
  test('switches tenants and keeps roster data isolated', async ({ page }) => {
    const state = makeState(makePlatformIdentity(), tenantA.id);
    await installTeamApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, tenantA);

    await page.goto('/tenant/team');
    await expect(page.getByRole('heading', { name: 'Team' })).toBeVisible();
    await expect(page.getByText('owner@acme.test')).toBeVisible();
    await expect(page.getByText('owner@northwind.test')).toHaveCount(0);

    await page.getByRole('button', { name: 'Switch tenant' }).click();
    await page.locator('.option').filter({ hasText: 'Northwind Help' }).click();
    await page.goto('/tenant/team');

    await expect(page.getByText('owner@northwind.test')).toBeVisible();
    await expect(page.getByText('owner@acme.test')).toHaveCount(0);
  });

  test('creates invitations and accepts them into the team page', async ({ page }) => {
    const state = makeState(makeOwnerIdentity(), tenantA.id);
    await installTeamApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, tenantA);

    await page.goto('/tenant/team');
    await page.getByRole('button', { name: 'Invite' }).click();
    await page.getByLabel('Email').fill('new-manager@acme.test');
    await page.getByRole('dialog').getByRole('button', { name: 'Manager' }).click();
    await page.getByRole('button', { name: 'Send invitation' }).click();

    await expect(page.getByRole('heading', { name: 'Invitation sent' })).toBeVisible();
    const invitation = state.invitationsByTenant[tenantA.id][0];
    await expect(page.getByText(invitation.token)).toBeVisible();

    state.currentUser = null;
    state.activeTenantId = null;
    await page.evaluate(() => localStorage.removeItem('app.tenant'));
    state.currentUser = {
      id: 'user-guest-accept',
      email: invitation.email,
      displayName: 'Nora Guest',
      platformRole: null,
      platformPermissions: [],
      staffTenantPermissions: null,
      memberships: [],
    };
    const guest = await page.context().newPage();
    await installTeamApi(guest, state);
    await guest.goto(`/invite/${invitation.token}`);
    await expect(guest.getByLabel('Display name')).toBeVisible();
    await guest.getByLabel('Display name').fill('Nora Manager');
    await guest.getByLabel('Password').fill('securePassword123!');
    await guest.getByRole('button', { name: 'Accept & join' }).click();

    await expect(guest).toHaveURL(/\/tenant\/team$/);
    await expect(guest.getByText('Nora Manager')).toBeVisible();

    const denyResponse = await page.evaluate(async () => {
      const res = await fetch('/api/v1/tenant/members/member-a-admin', {
        method: 'PATCH',
        headers: {
          'content-type': 'application/json',
          'X-Tenant-ID': 'tenant-a',
        },
        body: JSON.stringify({ role: 'owner' }),
      });
      return res.status;
    });
    expect(denyResponse).toBe(403);
  });

  test('changes roles and disables and re-enables members', async ({ page }) => {
    const state = makeState(makeOwnerIdentity(), tenantA.id);
    await installTeamApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, tenantA);

    await page.goto('/tenant/team');
    const agentRow = page.locator('tr').filter({ hasText: 'Ava Agent' });
    await expect(agentRow).toContainText('Agent');

    await agentRow.getByRole('button', { name: 'Manager' }).click();
    await expect(agentRow).toContainText('Manager');

    await agentRow.getByRole('button', { name: 'Disable' }).click();
    await expect(agentRow).toContainText('Disabled');

    await agentRow.getByRole('button', { name: 'Enable' }).click();
    await expect(agentRow).toContainText('Active');
  });

  test('rejects manager attempts to manage senior members', async ({ page }) => {
    const state = makeState(makeManagerIdentity(), tenantA.id);
    await installTeamApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, tenantA);

    await page.goto('/tenant/team');
    const adminRow = page.locator('tr').filter({ hasText: 'Alice Admin' });
    await expect(adminRow.getByRole('button', { name: 'Manager' })).toHaveCount(0);

    const responseStatus = await page.evaluate(async () => {
      const response = await fetch('/api/v1/tenant/members/member-a-admin', {
        method: 'PATCH',
        headers: {
          'content-type': 'application/json',
          'X-Tenant-ID': 'tenant-a',
        },
        body: JSON.stringify({ status: 'disabled' }),
      });
      return response.status;
    });
    expect(responseStatus).toBe(403);
  });
});
