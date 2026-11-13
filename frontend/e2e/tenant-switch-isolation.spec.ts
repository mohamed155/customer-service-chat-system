import { Page, Route } from '@playwright/test';
import { expect, test } from './support/test';

// MOH-61: switching the active workspace in place must never leave the old workspace's data on
// screen, not even while the new workspace's data is still loading, and a response that was
// already on its way for the old workspace must never land in the new one's stores.

const PERMISSIONS = ['overview.view', 'conversations.view', 'customers.view', 'settings.view'];

const tenantA = {
  id: 'tenant-a',
  name: 'Acme Support',
  slug: 'acme',
  status: 'active',
  plan: 'starter',
};
const tenantB = {
  id: 'tenant-b',
  name: 'Northwind Help',
  slug: 'northwind',
  status: 'active',
  plan: 'starter',
};

const platformAdmin = {
  id: 'platform-user',
  email: 'admin@helix.test',
  displayName: 'Priya Platform',
  platformRole: 'super_admin',
  platformPermissions: ['platform.admin', 'platform.tenants.list', 'platform.tenants.switch'],
  staffTenantPermissions: PERMISSIONS,
  memberships: [],
};

const conversation = (id: string, customer: string) => ({
  id,
  customer: { id: `cust-${id}`, display_name: customer },
  channel: 'web_chat',
  status: 'open',
  assignee: null,
  last_message: null,
  last_activity_at: '2026-07-09T09:00:00Z',
  created_at: '2026-07-09T08:00:00Z',
});

const CONVERSATIONS: Record<string, ReturnType<typeof conversation>[]> = {
  [tenantA.id]: [conversation('conv-a', 'Alma Acme')],
  [tenantB.id]: [conversation('conv-b', 'Bella Northwind')],
};

const queueEntry = (id: string, customer: string) => ({
  escalation: {
    id: `esc-${id}`,
    conversationId: `conv-${id}`,
    reason: 'customer_requested',
    requiredSkills: [],
    status: 'queued',
    routing: null,
    escalatedAt: '2026-07-09T09:00:00Z',
    closedAt: null,
  },
  conversation: {
    id: `conv-${id}`,
    channel: 'web_chat',
    customer: { id: `cust-${id}`, name: customer },
  },
  waitingSeconds: 120,
});

const QUEUES: Record<string, ReturnType<typeof queueEntry>[]> = {
  [tenantA.id]: [queueEntry('qa', 'Quentin Acme')],
  [tenantB.id]: [queueEntry('qb', 'Quinn Northwind')],
};

function json(route: Route, data: unknown) {
  return route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(data),
  });
}

/** Resolves when `release()` is called; used to hold a response. */
function gate() {
  let release!: () => void;
  const opened = new Promise<void>((resolve) => (release = resolve));
  return { opened, release };
}

async function installApi(
  page: Page,
  held: Partial<Record<string, Promise<void>>>,
  seen: { tenant: string | undefined }[],
) {
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    if (path === '/me') return json(route, platformAdmin);
    if (path === '/platform/tenants') {
      return json(route, { items: [tenantA, tenantB], nextCursor: null });
    }
    const switched = /^\/platform\/tenants\/([^/]+)\/switch$/.exec(path);
    if (switched) return json(route, switched[1] === tenantA.id ? tenantA : tenantB);
    if (path === '/tenant/conversations') {
      const tenant = route.request().headers()['x-tenant-id'];
      seen.push({ tenant });
      await held[tenant ?? '']; // the response is held until the test opens the gate
      const data = CONVERSATIONS[tenant ?? ''] ?? [];
      return json(route, { data, pagination: { next_cursor: null, has_more: false } }).catch(
        () => undefined, // the request was cancelled while it was held
      );
    }
    if (path === '/tenant/escalations/queue') {
      const tenant = route.request().headers()['x-tenant-id'];
      await held[`queue:${tenant ?? ''}`];
      const data = QUEUES[tenant ?? ''] ?? [];
      return json(route, { data, pagination: { next_cursor: null, has_more: false } }).catch(
        () => undefined,
      );
    }
    if (path === '/auth/logout') return json(route, null);
    return json(route, { items: [], nextCursor: null, hasMore: false });
  });
  // Seed the stored workspace on the first visit only; the switch below changes it in place.
  await page.addInitScript((tenant) => {
    if (!localStorage.getItem('app.tenant')) {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }
  }, tenantA);
}

async function switchToNorthwind(page: Page) {
  await page.getByRole('button', { name: 'Switch tenant' }).click();
  await page.locator('.option').filter({ hasText: tenantB.name }).click();
}

test.describe('switching workspace in place', () => {
  test('shows none of workspace A while workspace B is still loading', async ({ page }) => {
    const seen: { tenant: string | undefined }[] = [];
    const bGate = gate();
    await installApi(page, { [tenantB.id]: bGate.opened }, seen);

    await page.goto('/tenant/conversations');
    await expect(page.getByText('Alma Acme')).toBeVisible();

    const switched = page.waitForResponse((r) => r.url().includes('/switch'));
    await switchToNorthwind(page);
    await switched;

    // Workspace B's conversations are being held: A's must already be gone.
    await expect(page.getByText('Alma Acme')).toHaveCount(0);
    await expect(page.getByText('Bella Northwind')).toHaveCount(0);
    await expect(page.locator('app-loading-state')).toBeVisible();

    bGate.release();
    await expect(page.getByText('Bella Northwind')).toBeVisible();
    await expect(page.getByText('Alma Acme')).toHaveCount(0);
    expect(seen.map((s) => s.tenant)).toEqual([tenantA.id, tenantB.id]);
  });

  test('a response still on its way for workspace A never reaches workspace B', async ({
    page,
  }) => {
    const seen: { tenant: string | undefined }[] = [];
    const aGate = gate();
    await installApi(page, { [tenantA.id]: aGate.opened }, seen);

    await page.goto('/tenant/conversations');
    await expect.poll(() => seen.length).toBe(1); // A's list is in flight and held

    const switched = page.waitForResponse((r) => r.url().includes('/switch'));
    await switchToNorthwind(page);
    await switched;
    await expect(page.getByText('Bella Northwind')).toBeVisible();

    aGate.release(); // A's answer arrives late
    await page.waitForTimeout(300);

    await expect(page.getByText('Alma Acme')).toHaveCount(0);
    await expect(page.getByText('Bella Northwind')).toBeVisible();
  });
  test('a page that never reloaded on its own (escalation queue) is emptied and refilled', async ({
    page,
  }) => {
    const seen: { tenant: string | undefined }[] = [];
    const bGate = gate();
    await installApi(page, { [`queue:${tenantB.id}`]: bGate.opened }, seen);

    await page.goto('/tenant/escalations');
    await expect(page.getByText('Quentin Acme')).toBeVisible();

    const switched = page.waitForResponse((r) => r.url().includes('/switch'));
    await switchToNorthwind(page);
    await switched;

    await expect(page.getByText('Quentin Acme')).toHaveCount(0);

    bGate.release();
    await expect(page.getByText('Quinn Northwind')).toBeVisible();
    await expect(page.getByText('Quentin Acme')).toHaveCount(0);
  });
});
