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
  '/security',
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

  test('Trusted Types is required for scripts and only Angular’s policies are allowed', async () => {
    const directives = Object.fromEntries(
      csp.split(';').map((d) => {
        const [name, ...values] = d.trim().split(/\s+/);
        return [name, values];
      }),
    );
    expect(directives['require-trusted-types-for']).toEqual(["'script'"]);
    // No `default` policy and no `'allow-duplicates'`: a policy this app does not know about
    // cannot be created.
    expect(directives['trusted-types']).toEqual(['angular', 'angular#unsafe-bypass']);
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
    // Trusted Types alone would stop `script.textContent = '...'` before the CSP is asked, so make
    // the script through a policy the page allows. The CSP must still refuse to run it.
    await page.evaluate(() => {
      const tt = (
        window as unknown as {
          trustedTypes: {
            createPolicy(
              name: string,
              rules: { createScript(s: string): string },
            ): { createScript(s: string): unknown };
          };
        }
      ).trustedTypes;
      const policy = tt.createPolicy('angular#unsafe-bypass', { createScript: (s) => s });
      const script = document.createElement('script');
      script.textContent = policy.createScript('window.__ran = true') as string;
      document.head.appendChild(script);
    });
    await expect.poll(() => violations(page)).not.toEqual([]);
    expect(await violations(page)).toEqual([expect.stringContaining('script-src')]);
    expect(await page.evaluate(() => (window as unknown as { __ran?: boolean }).__ran)).toBe(
      undefined,
    );
  });

  test('a plain string written to an HTML sink is refused and reported', async ({ page }) => {
    await recordViolations(page);
    await mockSignedOut(page);
    await page.goto('/');
    const outcome = await page.evaluate(() => {
      const host = document.createElement('div');
      document.body.appendChild(host);
      const attempts: Record<string, string> = {};
      const attempt = (name: string, write: () => void) => {
        try {
          write();
          attempts[name] = 'allowed';
        } catch (error) {
          attempts[name] = (error as Error).name;
        }
      };
      attempt('innerHTML', () => (host.innerHTML = '<img src=x onerror=alert(1)>'));
      attempt('insertAdjacentHTML', () => host.insertAdjacentHTML('beforeend', '<b>x</b>'));
      attempt('iframe srcdoc', () => {
        const frame = document.createElement('iframe');
        frame.srcdoc = '<p>x</p>';
      });
      return { attempts, html: host.innerHTML };
    });
    expect(outcome.attempts).toEqual({
      innerHTML: 'TypeError',
      insertAdjacentHTML: 'TypeError',
      'iframe srcdoc': 'TypeError',
    });
    expect(outcome.html).toBe('');
    await expect
      .poll(async () =>
        (await violations(page)).filter((v) => v.startsWith('require-trusted-types-for')),
      )
      .toHaveLength(3);
  });

  // The knowledge article page binds `[innerHTML]` (reviewed in MOH-170). Angular sanitises the
  // value with its own Trusted Types policy: markup it allows renders and the browser reports
  // nothing.
  async function openArticle(page: Page, body: string) {
    await recordViolations(page);
    await mockSignedIn(page);
    await page.route('**/api/v1/tenant/knowledge/items/art-1', (route) =>
      json(route, {
        id: 'art-1',
        itemType: 'article',
        title: 'Refund policy',
        status: 'published',
        categoryId: null,
        categoryName: null,
        createdByDisplay: 'Olivia Owner',
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-02T00:00:00Z',
        tags: [],
        source: 'authored',
        createdByUserId: null,
        document: null,
        body,
      }),
    );
    await page.goto('/tenant/knowledge-base/art-1');
    await expect(page.locator('.body').getByRole('heading', { name: 'Refunds' })).toBeVisible();
  }

  test('an article body renders under Trusted Types with no violation', async ({ page }) => {
    await openArticle(page, '<h2>Refunds</h2><p>Within <strong>30 days</strong>.</p>');

    await expect(page.locator('.body strong')).toHaveText('30 days');
    expect(await violations(page)).toEqual([]);
  });

  // The server sanitises article bodies on write, so this markup never arrives in practice. It is
  // the second line: whatever does arrive is cleaned by Angular, nothing runs, and the sink is
  // never fed a string the policy did not make. (The browser may still report the inline handler
  // Angular drops while it parses the markup in an inert document; it is not executed.)
  test('a hostile article body is sanitised, not run, and never breaks Trusted Types', async ({
    page,
  }) => {
    await openArticle(
      page,
      '<h2>Refunds</h2><img src="x" onerror="window.__xss = true"><script>window.__xss = true</script><a href="javascript:window.__xss = true">x</a>',
    );

    const body = page.locator('.body');
    expect(await body.locator('script').count()).toBe(0);
    expect(await body.locator('[onerror]').count()).toBe(0);
    // Angular keeps the link but prefixes it, so the browser will not run it.
    expect(await body.locator('a').getAttribute('href')).toMatch(/^unsafe:/);
    expect(await page.evaluate(() => (window as unknown as { __xss?: boolean }).__xss)).toBe(
      undefined,
    );
    expect(
      (await violations(page)).filter((v) => v.startsWith('require-trusted-types-for')),
    ).toEqual([]);
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

  for (const route of [
    '/auth/login',
    '/auth/signup',
    '/auth/forgot-password',
    '/auth/mfa',
    '/no/such/page',
  ]) {
    test(`${route} renders with no violations`, async ({ page }) => {
      await recordViolations(page);
      await mockSignedOut(page);
      await page.goto(route);
      await page.waitForLoadState('networkidle');
      expect(await violations(page)).toEqual([]);
    });
  }

  test('/mfa-setup (a signed-in user setting up a second factor) renders with no violations', async ({
    page,
  }) => {
    await recordViolations(page);
    await mockSignedIn(page);
    await page.goto('/mfa-setup');
    await expect(page.getByText('Set up two-step verification').first()).toBeVisible();
    await page.waitForLoadState('networkidle');
    expect(new URL(page.url()).pathname).toBe('/mfa-setup');
    expect(await violations(page)).toEqual([]);
  });
});
