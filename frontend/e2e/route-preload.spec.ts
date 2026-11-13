import { Page, Route } from '@playwright/test';
import { expect, test } from './support/test';

// Route preloading (MOH-171): the inbox, a conversation and the customer list are marked
// `data: { preload: true }`, so the browser fetches them a few seconds after Overview has loaded, when idle. A
// route chunk is recognised by the component selector it contains (the minified build keeps it).

const permissions = [
  'overview.view',
  'conversations.view',
  'customers.view',
  'settings.view',
  'knowledge_base.view',
];
const identity = {
  id: 'tenant-user',
  email: 'owner@acme.test',
  displayName: 'Olivia Owner',
  platformRole: null,
  platformPermissions: [],
  staffTenantPermissions: null,
  memberships: [
    {
      tenantId: 'tenant-1',
      tenantName: 'Acme Support',
      tenantSlug: 'acme-support',
      role: 'owner',
      permissions,
    },
  ],
};

async function json(route: Route, data: unknown) {
  await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) });
}

/** Open Overview and report which route components' code the browser downloaded. */
async function openOverview(page: Page): Promise<() => Promise<Record<string, boolean>>> {
  const bodies: Promise<string>[] = [];
  page.on('response', (response) => {
    if (response.request().resourceType() === 'script') {
      bodies.push(response.text().catch(() => ''));
    }
  });
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    return json(route, path === '/me' ? identity : { items: [], nextCursor: null });
  });
  await page.goto('/tenant/overview');
  await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeVisible();
  return async () => {
    const code = (await Promise.all(bodies)).join('\n');
    return {
      detail: code.includes('app-conversation-detail'),
      inbox: code.includes('app-conversations'),
      customers: code.includes('app-customers'),
      // Not marked: it must wait until it is opened.
      knowledgeBase: code.includes('app-knowledge-base'),
    };
  };
}

test.describe('route preloading', () => {
  test('the inbox, a conversation and the customer list load while idle, nothing else', async ({
    page,
  }) => {
    const downloaded = await openOverview(page);

    await expect.poll(async () => (await downloaded()).detail, { timeout: 20_000 }).toBe(true);
    await page.waitForLoadState('networkidle');

    expect(await downloaded()).toEqual({
      detail: true,
      inbox: true,
      customers: true,
      knowledgeBase: false,
    });
  });

  test('a user who asked to save data downloads no route until opening it', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, 'connection', {
        value: { saveData: true, effectiveType: '4g' },
        configurable: true,
      });
    });
    const downloaded = await openOverview(page);
    await page.waitForLoadState('networkidle');
    // Idle callbacks have had plenty of time to run.
    await page.waitForTimeout(7_000);

    expect(await downloaded()).toEqual({
      detail: false,
      inbox: false,
      customers: false,
      knowledgeBase: false,
    });
  });

  test('opening a preloaded route works', async ({ page }) => {
    const downloaded = await openOverview(page);
    await expect.poll(async () => (await downloaded()).inbox, { timeout: 20_000 }).toBe(true);

    await page.getByRole('link', { name: 'Customers' }).first().click();

    await expect(page).toHaveURL(/\/tenant\/customers$/);
    await expect(page.getByRole('heading', { name: 'Customers' }).first()).toBeVisible();
  });
});
