import { expect, Page, Route, test } from '@playwright/test';
// eslint-disable-next-line @typescript-eslint/no-require-imports
const { dashboardCsp } = require('./support/csp.cjs') as { dashboardCsp: () => string };

// The static server sends the dashboard document with the same enforced Content-Security-Policy as
// the production edge (infra/Caddyfile). These specs fail on any violation, which is what a soak
// in report-only mode would otherwise have to find on staging (MOH-38).

const csp: string = dashboardCsp();

const permissions = ['overview.view', 'conversations.view', 'customers.view', 'settings.view'];
// Every permission the tenant routes ask for (core/authz/permissions.ts), so the guards let each
// page render instead of redirecting.
const allTenantPermissions = [
  ...permissions,
  'ai_agent.view',
  'knowledge_base.view',
  'integrations.view',
  'analytics.view',
  'tools.view',
  'members.view',
  'widgets.view',
  'audit.view',
  'billing.view',
];
const tenantRoutes = [
  '/tenant/overview',
  '/tenant/conversations',
  '/tenant/customers',
  '/tenant/ai-agent',
  '/tenant/ai-agent/prompt',
  '/tenant/knowledge-base',
  '/tenant/knowledge-base/new',
  '/tenant/integrations',
  '/tenant/analytics',
  '/tenant/audit-logs',
  '/tenant/billing',
  '/tenant/settings',
  '/tenant/settings/tools',
  '/tenant/team',
  '/tenant/escalations',
  '/tenant/widgets',
  '/tenant/notifications',
];
const platformPermissions = [
  'platform.admin',
  'platform.tenants.list',
  'platform.tenants.switch',
  'platform.tenants.manage',
  'platform.analytics.view',
  'platform.diagnostics.view',
  'platform.billing.view',
  'platform.audit.view',
];
const platformRoutes = [
  '/platform/overview-placeholder',
  '/platform/tenants',
  '/platform/tenants/new',
  '/platform/audit',
  '/platform/manage-billing',
  '/platform/system-health',
  '/platform/ai-providers',
];
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
      permissions: allTenantPermissions,
    },
  ],
};
const platformIdentity = {
  ...identity,
  id: 'platform-user',
  email: 'ops@platform.test',
  displayName: 'Pat Platform',
  platformRole: 'super_admin',
  platformPermissions,
  memberships: [],
};

async function json(route: Route, data: unknown, status = 200) {
  await route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

async function mockSignedIn(page: Page, me: unknown = identity) {
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    if (path === '/me') return json(route, me);
    return json(route, { items: [], nextCursor: null });
  });
}

async function mockSignedOut(page: Page) {
  await page.route('**/api/v1/**', (route) =>
    json(route, { error: { code: 'unauthenticated', message: 'Sign in', details: [] } }, 401),
  );
}

/** Record every CSP violation the browser reports, before any page script runs. */
async function recordViolations(page: Page) {
  await page.addInitScript(() => {
    const seen: string[] = [];
    (window as unknown as { __csp: string[] }).__csp = seen;
    document.addEventListener('securitypolicyviolation', (event) => {
      seen.push(`${event.violatedDirective} blocked ${event.blockedURI || 'inline'}`);
    });
  });
}

async function violations(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __csp?: string[] }).__csp ?? []);
}

test.describe('Content-Security-Policy enforcement', () => {
  test('the document is served with the enforced policy and no inline-script concession', async ({
    page,
  }) => {
    const response = await page.goto('/');
    const header = await response?.headerValue('content-security-policy');

    expect(header).toBe(csp);
    expect(await response?.headerValue('content-security-policy-report-only')).toBeNull();
    const directives = Object.fromEntries(
      csp.split(';').map((d) => {
        const [name, ...values] = d.trim().split(/\s+/);
        return [name, values];
      }),
    );
    expect(directives['script-src']).toEqual(["'self'"]);
    expect(directives['script-src']).not.toContain("'unsafe-inline'");
    expect(directives['object-src']).toEqual(["'none'"]);
    expect(directives['frame-ancestors']).toEqual(["'none'"]);
  });

  test('the built document needs no inline script or inline event handler', async ({ page }) => {
    await mockSignedOut(page);
    await page.goto('/');
    const inline = await page.evaluate(() => ({
      scripts: [...document.querySelectorAll('script:not([src])')].map((s) => s.textContent),
      handlers: [...document.querySelectorAll('*')]
        .flatMap((el) => [...el.attributes].map((a) => a.name))
        .filter((name) => name.startsWith('on')),
      printLinks: document.querySelectorAll('link[rel="stylesheet"][media="print"]').length,
    }));
    expect(inline.scripts).toEqual([]);
    expect(inline.handlers).toEqual([]);
    expect(inline.printLinks).toBe(0);
  });

  test('the harness really does report a blocked inline script', async ({ page }) => {
    await recordViolations(page);
    await mockSignedOut(page);
    await page.goto('/');
    await page.evaluate(() => {
      const script = document.createElement('script');
      script.textContent = 'window.__ran = true';
      document.head.appendChild(script);
    });
    await expect.poll(() => violations(page)).not.toEqual([]);
    expect(await page.evaluate(() => (window as unknown as { __ran?: boolean }).__ran)).toBe(
      undefined,
    );
  });

  test('the signed-out page loads, is styled and applies the saved theme with no violations', async ({
    page,
  }) => {
    await recordViolations(page);
    await mockSignedOut(page);
    await page.addInitScript(() => localStorage.setItem('app.themeMode', 'dark'));
    await page.goto('/');

    // theme-init.js is an external script, so it still runs under `script-src 'self'`.
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    // The main stylesheet applies to screen media and has rules.
    const styled = await page.evaluate(() => {
      const sheets = [...document.styleSheets].filter((s) => (s.href ?? '').includes('styles-'));
      return (
        sheets.length > 0 && sheets.every((s) => s.cssRules.length > 0 && s.media.length === 0)
      );
    });
    expect(styled).toBe(true);
    expect(await violations(page)).toEqual([]);
  });

  // MOH-38 asked for the report-only violation reports to be reviewed before enforcing. There is no
  // report endpoint, so this is the review: every dashboard route, run under the enforced policy,
  // fails on any violation. Add a route here when you add one to the router.
  for (const route of tenantRoutes) {
    test(`${route} renders with no violations`, async ({ page }) => {
      await recordViolations(page);
      await mockSignedIn(page);
      await page.goto(route);
      await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeVisible();
      await page.waitForLoadState('networkidle');
      expect(new URL(page.url()).pathname).toBe(route);
      expect(await violations(page)).toEqual([]);
    });
  }

  for (const route of platformRoutes) {
    test(`${route} renders with no violations`, async ({ page }) => {
      await recordViolations(page);
      await mockSignedIn(page, platformIdentity);
      await page.goto(route);
      await expect(page.getByRole('banner')).toBeVisible();
      await page.waitForLoadState('networkidle');
      expect(new URL(page.url()).pathname).toBe(route);
      expect(await violations(page)).toEqual([]);
    });
  }

  for (const route of ['/auth/login', '/auth/signup', '/auth/forgot-password', '/no/such/page']) {
    test(`${route} renders with no violations`, async ({ page }) => {
      await recordViolations(page);
      await mockSignedOut(page);
      await page.goto(route);
      await page.waitForLoadState('networkidle');
      expect(await violations(page)).toEqual([]);
    });
  }
});
