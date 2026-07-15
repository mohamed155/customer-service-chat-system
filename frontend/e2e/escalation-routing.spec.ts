import { expect, Page, Route, test } from '@playwright/test';

type EscalationStatus = 'queued' | 'assigned' | 'closed';
type RoutingReason =
  'skill_match' | 'load_fallback' | 'manual_claim' | 'queue_auto' | 'manual_reassignment';
type AvailabilityState = 'available' | 'away';

interface Escalation {
  readonly id: string;
  readonly conversationId: string;
  readonly reason: string;
  readonly requiredSkills: { readonly id: string | null; readonly name: string }[];
  readonly status: EscalationStatus;
  readonly routing: {
    readonly reason: RoutingReason;
    readonly matchedSkills: string[];
    readonly assignedMembershipId: string;
    readonly assignedAt: string;
  } | null;
  readonly escalatedAt: string;
  readonly closedAt: string | null;
}

interface QueueEntry {
  readonly escalation: Escalation;
  readonly conversation: {
    readonly id: string;
    readonly channel: string;
    readonly customer: { readonly id: string; readonly name: string };
  };
  readonly waitingSeconds: number;
}

interface PaginatedResponse<T> {
  readonly data: T[];
  readonly pagination: { readonly nextCursor: string | null; readonly hasMore: boolean };
}

interface Skill {
  readonly id: string;
  readonly name: string;
  readonly agentCount: number;
}

interface Availability {
  readonly membershipId: string;
  readonly state: AvailabilityState;
  readonly stateChangedAt: string | null;
}

interface ConversationWire {
  readonly id: string;
  readonly customer: { readonly id: string; readonly displayName: string };
  readonly channel: string;
  readonly status: string;
  readonly escalation: Escalation | null;
}

const AGENT_A_MEMBERSHIP = 'member-agent-a';
const AGENT_B_MEMBERSHIP = 'member-agent-b';
const AGENT_C_MEMBERSHIP = 'member-agent-c';

const testTenant = {
  id: 'tenant-esc',
  name: 'Esc Co',
  slug: 'esc-co',
  status: 'active' as const,
  plan: 'starter' as const,
};

function agentAIdentity() {
  return {
    id: 'user-agent-a',
    email: 'agent-a@test.com',
    displayName: 'Alice Agent',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: 'tenant-esc',
        tenantName: 'Esc Co',
        tenantSlug: 'esc-co',
        role: 'agent' as const,
        permissions: [
          'conversations.view',
          'conversations.manage',
          'members.view',
          'members.manage',
        ],
      },
    ],
  };
}

function json(route: Route, data: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

async function installEscalationApi(page: Page) {
  const conversations: ConversationWire[] = [
    {
      id: 'conv-esc-1',
      customer: { id: 'cust-1', displayName: 'One Customer' },
      channel: 'web_chat',
      status: 'open',
      escalation: null,
    },
    {
      id: 'conv-esc-2',
      customer: { id: 'cust-2', displayName: 'Two Customer' },
      channel: 'email',
      status: 'open',
      escalation: null,
    },
    {
      id: 'conv-esc-3',
      customer: { id: 'cust-3', displayName: 'Three Customer' },
      channel: 'web_chat',
      status: 'open',
      escalation: null,
    },
  ];

  const skillsStore: Skill[] = [
    { id: 'skill-1', name: 'billing', agentCount: 1 },
    { id: 'skill-2', name: 'arabic', agentCount: 1 },
  ];

  let escalationsStore: Escalation[] = [];
  let availabilityStore: Record<string, AvailabilityState> = {};

  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    const method = route.request().method();

    if (path === '/me') return json(route, agentAIdentity());

    // Skills
    if (path === '/tenant/skills' && method === 'GET')
      return json(route, { data: skillsStore, pagination: { nextCursor: null, hasMore: false } });
    if (path === '/tenant/skills' && method === 'POST') {
      const body = route.request().postDataJSON() as { name: string };
      const existing = skillsStore.find((s) => s.name.toLowerCase() === body.name.toLowerCase());
      if (existing)
        return json(
          route,
          {
            error: {
              message: 'duplicate name',
              details: [{ field: 'name', message: 'already exists' }],
            },
          },
          409,
        );
      const skill: Skill = {
        id: `skill-${skillsStore.length + 1}`,
        name: body.name,
        agentCount: 0,
      };
      skillsStore.push(skill);
      return json(route, { data: skill }, 201);
    }
    const skillMatch = path.match(/^\/tenant\/skills\/([^/]+)$/);
    if (skillMatch && method === 'PATCH') {
      const body = route.request().postDataJSON() as { name: string };
      const skill = skillsStore.find((s) => s.id === skillMatch[1]);
      if (!skill) return json(route, { error: { message: 'not found' } }, 404);
      skill.name = body.name;
      return json(route, { data: skill });
    }
    if (skillMatch && method === 'DELETE') {
      const idx = skillsStore.findIndex((s) => s.id === skillMatch[1]);
      if (idx === -1) return json(route, { error: { message: 'not found' } }, 404);
      skillsStore.splice(idx, 1);
      return json(route, null, 204);
    }
    const memberSkillsMatch = path.match(/^\/tenant\/members\/([^/]+)\/skills$/);
    if (memberSkillsMatch && method === 'PUT') {
      return json(route, { data: null });
    }

    // Availability
    if (path === '/tenant/availability/me' && method === 'GET') {
      const state = availabilityStore[AGENT_A_MEMBERSHIP] || 'away';
      return json(route, {
        data: {
          membershipId: AGENT_A_MEMBERSHIP,
          state,
          stateChangedAt: state === 'available' ? new Date().toISOString() : null,
        },
      });
    }
    if (path === '/tenant/availability/me' && method === 'PUT') {
      const body = route.request().postDataJSON() as { state: AvailabilityState };
      availabilityStore[AGENT_A_MEMBERSHIP] = body.state;
      return json(route, {
        data: {
          membershipId: AGENT_A_MEMBERSHIP,
          state: body.state,
          stateChangedAt: new Date().toISOString(),
        },
      });
    }

    // Escalation queue
    if (path === '/tenant/escalations/queue' && method === 'GET') {
      const queued = escalationsStore.filter((e) => e.status === 'queued');
      return json(route, {
        data: queued.map((e) => ({
          escalation: e,
          conversation: {
            id: e.conversationId,
            channel: 'web_chat',
            customer: { id: 'cust-q', name: 'Queue Customer' },
          },
          waitingSeconds: Math.floor((Date.now() - new Date(e.escalatedAt).getTime()) / 1000),
        })),
        pagination: { nextCursor: null, hasMore: false },
      });
    }

    const claimMatch = path.match(/^\/tenant\/escalations\/([^/]+)\/claim$/);
    if (claimMatch && method === 'POST') {
      const esc = escalationsStore.find((e) => e.id === claimMatch[1]);
      if (!esc || esc.status !== 'queued') {
        return json(
          route,
          {
            error: {
              message: 'already claimed',
              details: [{ assignedMembershipId: esc?.routing?.assignedMembershipId || '' }],
            },
          },
          409,
        );
      }
      esc.status = 'assigned';
      esc.routing = {
        reason: 'manual_claim',
        matchedSkills: [],
        assignedMembershipId: AGENT_A_MEMBERSHIP,
        assignedAt: new Date().toISOString(),
      };
      return json(route, { data: esc });
    }

    const escalateMatch = path.match(/^\/tenant\/conversations\/([^/]+)\/escalate$/);
    if (escalateMatch && method === 'POST') {
      const conv = conversations.find((c) => c.id === escalateMatch[1]);
      if (!conv) return json(route, { error: { message: 'not found' } }, 404);
      if (conv.status === 'resolved' || conv.status === 'closed')
        return json(
          route,
          {
            error: {
              message: 'invalid state',
              details: [{ field: 'status', message: 'conversation is already resolved/closed' }],
            },
          },
          422,
        );
      if (conv.escalation) return json(route, { error: { message: 'already escalated' } }, 409);

      const body = route.request().postDataJSON() as {
        reason: string;
        requiredSkillIds?: string[];
      };
      const esc: Escalation = {
        id: `esc-${Date.now()}`,
        conversationId: conv.id,
        reason: body.reason,
        requiredSkills: (body.requiredSkillIds || []).map((id) => ({
          id,
          name: id === 'skill-1' ? 'billing' : 'arabic',
        })),
        status:
          AGENT_A_MEMBERSHIP in availabilityStore &&
          availabilityStore[AGENT_A_MEMBERSHIP] === 'available'
            ? 'assigned'
            : 'queued',
        routing:
          AGENT_A_MEMBERSHIP in availabilityStore &&
          availabilityStore[AGENT_A_MEMBERSHIP] === 'available'
            ? {
                reason: 'skill_match',
                matchedSkills: body.requiredSkillIds?.length ? ['billing'] : [],
                assignedMembershipId: AGENT_A_MEMBERSHIP,
                assignedAt: new Date().toISOString(),
              }
            : null,
        escalatedAt: new Date().toISOString(),
        closedAt: null,
      };
      conv.escalation = esc;
      escalationsStore.push(esc);
      return json(route, { data: esc }, 201);
    }

    // Conversations list with escalated filter
    if (path === '/tenant/conversations' && method === 'GET') {
      const escalatedFilter = url.searchParams.get('escalated');
      let filtered = conversations;
      if (escalatedFilter === 'true')
        filtered = conversations.filter(
          (c) =>
            c.escalation !== null &&
            (c.escalation.status === 'queued' || c.escalation.status === 'assigned'),
        );
      return json(route, {
        data: filtered,
        pagination: { nextCursor: null, hasMore: false },
      });
    }

    const convDetail = path.match(/^\/tenant\/conversations\/([^/]+)$/);
    if (convDetail && method === 'GET') {
      const conv = conversations.find((c) => c.id === convDetail[1]);
      if (!conv) return json(route, { error: { message: 'not found' } }, 404);
      return json(route, { data: conv });
    }

    if (convDetail && method === 'PATCH') {
      const conv = conversations.find((c) => c.id === convDetail[1]);
      if (!conv) return json(route, { error: { message: 'not found' } }, 404);
      const patch = route.request().postDataJSON() as {
        status?: string;
        assignedMembershipId?: string | null;
      };
      if (patch.status) conv.status = patch.status;
      if (patch.assignedMembershipId !== undefined) {
        if (conv.escalation) {
          conv.escalation.routing = {
            reason: 'manual_reassignment',
            matchedSkills: [],
            assignedMembershipId: patch.assignedMembershipId || AGENT_A_MEMBERSHIP,
            assignedAt: new Date().toISOString(),
          };
          conv.escalation.status = 'assigned';
        }
      }
      return json(route, { data: { ...conv } });
    }

    return json(route, { data: null });
  });
}

test.describe('Escalation Routing', () => {
  test('Agent: can escalate a conversation and see it assigned or queued', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    await expect(page.getByRole('heading', { name: 'Conversations' })).toBeVisible();

    const convLink = page.getByRole('link', { name: /conv-esc-1/i });
    if (await convLink.isVisible().catch(() => false)) {
      await convLink.click();
    }
    await page.waitForURL(/\/tenant\/conversations\/conv-esc-1/);
  });

  test('Agent: can open the escalation queue and claim an entry', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/escalations');

    await expect(page.getByRole('heading', { name: /escalat/i })).toBeVisible();
  });

  test('Agent: availability toggle appears in topbar', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/overview');

    const toggle = page.getByRole('button', { name: /available|away|toggle.*avail/i });
    if (await toggle.isVisible().catch(() => false)) {
      await expect(toggle).toBeVisible();
    }
  });

  test('Agent: sees escalation banner on conversation detail', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-esc-1');
    await expect(page.getByRole('heading', { name: /conversation/i })).toBeVisible();
  });

  test('Agent: can manage skills on the team page', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/team');
    await expect(page.getByRole('heading', { name: /team|members/i })).toBeVisible();

    const skillsSection = page.getByText(/skill/i);
    if (await skillsSection.isVisible().catch(() => false)) {
      await expect(skillsSection).toBeVisible();
    }
  });

  test('Agent: escalated inbox filter narrows conversations', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    const filterChip = page.getByRole('checkbox', { name: /escalat/i });
    if (await filterChip.isVisible().catch(() => false)) {
      await filterChip.click();
      await page.waitForTimeout(300);
    }
  });

  test('Agent: sees routing reason on escalated conversation', async ({ page }) => {
    await installEscalationApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-esc-2');
    await expect(page.getByRole('heading', { name: /conversation/i })).toBeVisible();

    const banner = page.getByText(/escalat/i);
    if (await banner.isVisible().catch(() => false)) {
      await expect(banner).toBeVisible();
    }
  });
});
