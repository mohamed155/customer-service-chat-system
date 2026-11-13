import { expect, Page, Route, test } from '@playwright/test';

// MOH-168: an error nobody handled in a signed-in user's browser is sent to the backend, scrubbed,
// batched and with the page's own context (route pattern, release, workspace).

const PERMISSIONS = ['overview.view', 'conversations.view', 'customers.view', 'settings.view'];
const tenant = {
  id: '0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f',
  name: 'Acme Support',
  slug: 'acme-support',
  status: 'active',
};
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

interface Captured {
  headers: Record<string, string>;
  body: { reports: Record<string, unknown>[] };
}

function json(route: Route, data: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

/** Mock the API; `signedIn: false` answers /me with 401. Returns what the report endpoint got. */
async function mockApi(page: Page, signedIn: boolean): Promise<Captured[]> {
  const captured: Captured[] = [];
  await page.route('**/api/v1/**', async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname.replace('/api/v1', '');
    if (path === '/client-errors') {
      captured.push({ headers: request.headers(), body: request.postDataJSON() });
      return route.fulfill({ status: 204 });
    }
    if (path === '/me') {
      return signedIn
        ? json(route, identity)
        : json(route, { error: { code: 'unauthenticated', message: 'x', details: [] } }, 401);
    }
    return json(route, { items: [], data: [], nextCursor: null, hasMore: false });
  });
  return captured;
}

test.describe('browser error reports', () => {
  test('an uncaught error and an unhandled rejection reach the backend, scrubbed, in one batch', async ({
    page,
  }) => {
    const captured = await mockApi(page, true);
    await page.goto('/tenant/overview');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );

    await page.evaluate(() => {
      setTimeout(() => {
        throw new Error('Boom for jane@example.com at https://app.test/x?token=abc123');
      }, 0);
      void Promise.reject(new Error('Rejected with password=hunter2'));
    });

    await expect.poll(() => captured.length, { timeout: 15_000 }).toBe(1);

    const [request] = captured;
    expect(request.headers['x-requested-with']).toBe('XMLHttpRequest');
    const reports = request.body.reports;
    expect(reports.map((r) => r['kind']).sort()).toEqual(['error', 'unhandled_rejection']);
    for (const report of reports) {
      expect(report['route']).toBe('tenant/overview');
      expect(report['release']).toMatch(/^[A-Za-z0-9_-]{8}$/);
      expect(report['tenant_id']).toBe(tenant.id);
      // Nothing but the documented fields leaves the browser.
      expect(Object.keys(report).sort()).toEqual(
        ['kind', 'message', 'release', 'route', 'stack', 'tenant_id'].filter(
          (key) => key in report,
        ),
      );
    }
    const text = JSON.stringify(request.body);
    expect(text).not.toContain('jane@example.com');
    expect(text).not.toContain('token=abc123');
    expect(text).not.toContain('hunter2');
    expect(text).toContain('Boom for [email]');
  });

  test('a signed-out page sends nothing: the endpoint needs a session', async ({ page }) => {
    const captured = await mockApi(page, false);
    await page.goto('/auth/login');

    await page.evaluate(() => {
      setTimeout(() => {
        throw new Error('Boom while signed out');
      }, 0);
    });
    await page.waitForTimeout(6_500); // longer than the five seconds errors wait to be sent

    expect(captured).toHaveLength(0);
  });
});
