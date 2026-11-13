import { expect, Page, Route, test } from '@playwright/test';

// MOH-167: a page opened before a deploy still has the old main bundle, whose lazy routes point at
// files the new release no longer serves (a 404, as infra/nginx/spa.conf answers). Going to an area
// not visited yet must load the page again, once, and never loop.

const PERMISSIONS = ['overview.view', 'conversations.view', 'customers.view', 'settings.view'];
const tenant = { id: 'tenant-1', name: 'Acme Support', slug: 'acme-support', status: 'active' };
const identity = {
  id: 'tenant-user',
  email: 'owner@acme.test',
  displayName: 'Olivia Owner',
  platformRole: null,
  platformPermissions: [],
  staffTenantPermissions: null,
  memberships: [
    {
      tenantId: tenant.id,
      tenantName: tenant.name,
      tenantSlug: tenant.slug,
      role: 'owner',
      permissions: PERMISSIONS,
    },
  ],
};

function json(route: Route, data: unknown) {
  return route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(data),
  });
}

async function mockApi(page: Page) {
  await page.route('**/api/v1/**', (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    if (path === '/me') return json(route, identity);
    return json(route, { items: [], data: [], nextCursor: null, hasMore: false });
  });
}

/**
 * From now on every script the page has not loaded yet answers 404, the way the old release's
 * chunks do after a deploy. With `untilReload` the files come back at the next page load (the new
 * release's chunk names are served), otherwise they stay missing.
 */
async function removeUnloadedChunks(page: Page, loaded: Set<string>, untilReload: boolean) {
  let gone = true;
  if (untilReload) {
    page.on('request', (request) => {
      if (request.isNavigationRequest()) gone = false;
    });
  }
  await page.route('**/*.js', (route) => {
    const url = route.request().url();
    if (gone && !loaded.has(url)) {
      return route.fulfill({ status: 404, contentType: 'text/plain', body: 'Not found' });
    }
    return route.continue();
  });
}

async function openOverview(page: Page) {
  const loaded = new Set<string>();
  page.on('response', (response) => {
    if (response.url().endsWith('.js')) loaded.add(response.url());
  });
  await mockApi(page);
  await page.goto('/tenant/overview');
  await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
    'Customers',
  );
  return loaded;
}

test.describe('lazy chunks removed by a deploy', () => {
  test('loads the target URL again and then shows the page', async ({ page }) => {
    const loaded = await openOverview(page);
    await removeUnloadedChunks(page, loaded, true);
    const documentLoads: string[] = [];
    page.on('request', (request) => {
      if (request.isNavigationRequest()) documentLoads.push(new URL(request.url()).pathname);
    });

    await page.getByRole('link', { name: 'Customers' }).click();

    await expect(page).toHaveURL(/\/tenant\/customers$/);
    await expect(page.locator('app-customers')).toBeAttached();
    expect(documentLoads).toEqual(['/tenant/customers']);
    await expect(page.getByText('A new version is available')).toHaveCount(0);
  });

  test('does not reload in a loop when the chunk stays missing: shows the update notice', async ({
    page,
  }) => {
    const loaded = await openOverview(page);
    await removeUnloadedChunks(page, loaded, false);
    const documentLoads: string[] = [];
    page.on('request', (request) => {
      if (request.isNavigationRequest()) documentLoads.push(new URL(request.url()).pathname);
    });

    await page.getByRole('link', { name: 'Customers' }).click();

    await expect(page.getByRole('status')).toContainText('A new version is available');
    // One reload, then the guard held: the notice replaced a second one.
    expect(documentLoads).toEqual(['/tenant/customers']);
    await expect(page.getByRole('button', { name: 'Reload' })).toBeVisible();
  });

  test('a page whose release is still current shows no notice', async ({ page }) => {
    await openOverview(page);
    await page.waitForTimeout(500);
    await expect(page.getByText('A new version is available')).toHaveCount(0);
  });
});
