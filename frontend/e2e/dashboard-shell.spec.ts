import { expect, Page, Route, test } from '@playwright/test';

const TENANT_PERMISSIONS = [
  'overview.view',
  'conversations.view',
  'customers.view',
  'ai_agent.view',
  'knowledge_base.view',
  'integrations.view',
  'analytics.view',
  'settings.view',
] as const;

const tenant = { id: 'tenant-1', name: 'Acme Support', slug: 'acme-support', status: 'active' };

const identities = {
  tenant: {
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
        permissions: TENANT_PERMISSIONS,
      },
    ],
  },
  platform: {
    id: 'platform-user',
    email: 'admin@helix.test',
    displayName: 'Priya Platform',
    platformRole: 'super_admin',
    platformPermissions: ['platform.admin', 'platform.tenants.list', 'platform.tenants.switch'],
    staffTenantPermissions: TENANT_PERMISSIONS,
    memberships: [],
  },
  noRole: {
    id: 'no-role-user',
    email: 'new@helix.test',
    displayName: 'No Role',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [],
  },
} as const;

async function mockApi(page: Page, identity: (typeof identities)[keyof typeof identities]) {
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');

    if (path === '/me') return json(route, identity);
    if (path === '/platform/tenants') return json(route, { items: [tenant], nextCursor: null });
    if (path === `/platform/tenants/${tenant.id}/switch`) return json(route, tenant);
    if (path === '/auth/logout') return json(route, null);
    return json(route, { items: [], nextCursor: null });
  });
}

async function json(route: Route, data: unknown) {
  await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) });
}

test.describe('dashboard shell', () => {
  test('renders tenant, platform, and no-role identities with safe role-appropriate surfaces', async ({
    page,
  }) => {
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Platform' })).toHaveCount(0);
    await page.getByRole('button', { name: 'User menu' }).click();
    await expect(page.getByRole('menu')).toContainText('Olivia Owner');
    await expect(page.getByRole('menu')).toContainText('Owner · Acme Support');

    await page.unrouteAll({ behavior: 'wait' });
    await page.evaluate(() => localStorage.removeItem('app.tenant'));
    await mockApi(page, identities.platform);
    await page.goto('/tenant/select');
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Platform' })).toBeVisible();
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeEmpty();

    await page.unrouteAll({ behavior: 'wait' });
    await mockApi(page, identities.noRole);
    await page.goto('/tenant/select');
    await expect(page.getByRole('heading', { name: 'No workspace access' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'User menu' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toHaveCount(0);
  });

  test('switches a platform user into a tenant and enables workspace navigation', async ({
    page,
  }) => {
    await mockApi(page, identities.platform);
    await page.goto('/tenant/select');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeEmpty();

    await page.getByRole('button', { name: 'Switch tenant' }).click();
    await page.locator('.option').filter({ hasText: 'Acme Support' }).click();

    await expect(page.getByRole('button', { name: 'Switch tenant' })).toContainText('Acme Support');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
  });

  test('updates breadcrumbs through real navigation', async ({ page }) => {
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await page.getByRole('link', { name: 'Settings' }).click();

    await expect(page).toHaveURL(/\/tenant\/settings$/);
    const breadcrumb = page.getByRole('navigation', { name: 'Breadcrumb' });
    await expect(breadcrumb).toHaveText(/Workspace\s*Settings/);
    await expect(breadcrumb.getByText('Settings')).toHaveAttribute('aria-current', 'page');
  });

  test('dismisses the mobile drawer by scrim, Escape, and navigation', async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 800 });
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    const toggle = page.getByRole('button', { name: 'Toggle sidebar' });
    const drawer = page.locator('.sidebar-wrapper');

    await toggle.click();
    await expect(drawer).toHaveClass(/open/);
    await page.getByRole('button', { name: 'Close navigation drawer' }).click();
    await expect(drawer).not.toHaveClass(/open/);

    await toggle.click();
    await page.keyboard.press('Escape');
    await expect(drawer).not.toHaveClass(/open/);

    await toggle.click();
    await page.getByRole('link', { name: 'Conversations' }).click();
    await expect(page).toHaveURL(/\/tenant\/conversations$/);
    await expect(drawer).not.toHaveClass(/open/);
  });

  test('has no 360px overflow and essential platform controls activate', async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 800 });
    await page.addInitScript(() => localStorage.setItem('app.themeMode', 'light'));
    await mockApi(page, identities.platform);
    await page.goto('/tenant/select');

    const sizes = await page.evaluate(() => ({
      documentClient: document.documentElement.clientWidth,
      documentScroll: document.documentElement.scrollWidth,
      bodyClient: document.body.clientWidth,
      bodyScroll: document.body.scrollWidth,
    }));
    expect(sizes.documentScroll).toBeLessThanOrEqual(sizes.documentClient);
    expect(sizes.bodyScroll).toBeLessThanOrEqual(sizes.bodyClient);

    await page.getByRole('button', { name: 'Platform' }).click();
    await expect(page.getByRole('menuitem', { name: 'Platform overview' })).toBeVisible();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Switch tenant' }).click();
    await expect(page.getByPlaceholder('Search tenants...')).toBeVisible();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: /^Theme is/ }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await page.getByRole('button', { name: 'User menu' }).click();
    await expect(page.getByRole('menu')).toContainText('Priya Platform');
  });

  test('keeps the static skeleton until delayed identity resolves without entitlement flash', async ({
    page,
  }) => {
    let releaseIdentity!: () => void;
    const identityReleased = new Promise<void>((resolve) => (releaseIdentity = resolve));
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname;
      if (path.endsWith('/me')) {
        await identityReleased;
        return json(route, identities.platform);
      }
      return json(route, { items: [tenant], nextCursor: null });
    });

    const navigation = page.goto('/tenant/select');
    await expect(page.locator('.skeleton-shell')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Platform' })).toHaveCount(0);
    releaseIdentity();
    await navigation;

    await expect(page.locator('.skeleton-shell')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Platform' })).toBeVisible();
  });

  test('supports persisted light/dark and follows live system theme changes', async ({ page }) => {
    await mockApi(page, identities.tenant);
    await page.addInitScript(() => localStorage.setItem('app.themeMode', 'dark'));
    await page.goto('/tenant/overview');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

    await page.getByRole('button', { name: 'Theme is dark; switch to system' }).click();
    await page.emulateMedia({ colorScheme: 'light' });
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await page.emulateMedia({ colorScheme: 'dark' });
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

    await page.getByRole('button', { name: 'Theme is system; switch to light' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await expect
      .poll(() => page.evaluate(() => localStorage.getItem('app.themeMode')))
      .toBe('light');

    // Verify dark mode applies computed styling to shell surfaces
    await page.getByRole('button', { name: 'Theme is light; switch to dark' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    const sidebarBg = await page.evaluate(() => {
      const el = document.querySelector('app-sidebar');
      return el ? window.getComputedStyle(el).backgroundColor : null;
    });
    expect(sidebarBg).not.toBe('rgb(255, 255, 255)');
    expect(sidebarBg).not.toBe('rgba(0, 0, 0, 0)');
    const headerBg = await page.evaluate(() => {
      const el = document.querySelector('app-topbar header');
      return el ? window.getComputedStyle(el).backgroundColor : null;
    });
    expect(headerBg).not.toBe('rgb(255, 255, 255)');
    expect(headerBg).not.toBe('rgba(0, 0, 0, 0)');
  });

  test('activates breadcrumb ancestors and verifies navigation', async ({ page }) => {
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/settings');
    const breadcrumb = page.getByRole('navigation', { name: 'Breadcrumb' });
    await expect(breadcrumb).toContainText('Workspace');
    await expect(breadcrumb).toContainText('Settings');
    await breadcrumb.getByText('Workspace').click();
    await expect(page).toHaveURL(/\/tenant\/overview$/);
  });

  test('activates platform destinations navigating to overview-placeholder', async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 800 });
    await mockApi(page, identities.platform);
    await page.goto('/tenant/select');
    await page.getByRole('button', { name: 'Platform' }).click();
    await page.getByRole('menuitem', { name: 'Platform overview' }).click();
    await expect(page).toHaveURL(/\/platform\/overview-placeholder$/);
  });

  test('logs out successfully and redirects to sign-in', async ({ page }) => {
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await page.getByRole('button', { name: 'User menu' }).click();
    await page.locator('.sign-out').click();
    await expect(page).toHaveURL(/\/auth\/login$/);
  });

  test('handles failed logout and still navigates to sign-in clearing local state', async ({
    page,
  }) => {
    await page.route('**/api/v1/auth/logout', async (route) => {
      await route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ code: 'server_error', message: 'Server error', status: 500 }),
      });
    });
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await page.getByRole('button', { name: 'User menu' }).click();
    await page.locator('.sign-out').click();
    await expect(page).toHaveURL(/\/auth\/login$/);
    const tenantStorage = await page.evaluate(() => localStorage.getItem('app.tenant'));
    expect(tenantStorage).toBeNull();
  });

  test('shows skeleton during delayed identity then handles tenant switch without stale content', async ({
    page,
  }) => {
    let releaseIdentity!: () => void;
    const identityReleased = new Promise<void>((resolve) => (releaseIdentity = resolve));
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      if (path === '/me') {
        await identityReleased;
        return json(route, identities.platform);
      }
      if (path === '/platform/tenants') return json(route, { items: [tenant], nextCursor: null });
      if (path === `/platform/tenants/${tenant.id}/switch`) return json(route, tenant);
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null });
    });

    const navigation = page.goto('/tenant/select');
    await expect(page.locator('.skeleton-shell')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Platform' })).toHaveCount(0);
    releaseIdentity();
    await navigation;
    await expect(page.locator('.skeleton-shell')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Platform' })).toBeVisible();

    await page.getByRole('button', { name: 'Switch tenant' }).click();
    await page.locator('.option').filter({ hasText: 'Acme Support' }).click();
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toContainText('Acme Support');
    await expect(page.locator('.skeleton-shell')).toHaveCount(0);
  });

  test('preserves shell state through authenticated page refresh without entitlement flash', async ({
    page,
  }) => {
    await page.addInitScript(() =>
      localStorage.setItem(
        'app.tenant',
        JSON.stringify({
          id: 'tenant-1',
          name: 'Acme Support',
          slug: 'acme-support',
          status: 'active',
        }),
      ),
    );
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );

    await page.reload();

    await expect(page.locator('.skeleton-shell')).toHaveCount(0);
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
    await expect(page.getByRole('button', { name: 'User menu' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'User menu' })).toBeEnabled();
  });

  test('delays identity during refresh to verify skeleton and no entitlement flash', async ({
    page,
  }) => {
    let holdMe = false;
    let releaseMe!: () => void;
    const meReleased = new Promise<void>((resolve) => (releaseMe = resolve));

    await page.addInitScript(() =>
      localStorage.setItem(
        'app.tenant',
        JSON.stringify({
          id: 'tenant-1',
          name: 'Acme Support',
          slug: 'acme-support',
          status: 'active',
        }),
      ),
    );

    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      if (path === '/me') {
        if (holdMe) {
          await meReleased;
        }
        return json(route, identities.tenant);
      }
      if (path === '/platform/tenants') return json(route, { items: [tenant], nextCursor: null });
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/tenant/overview');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );

    holdMe = true;
    const reload = page.reload();
    await expect(page.locator('.skeleton-shell')).toBeVisible();
    await expect(page.getByRole('button', { name: 'User menu' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toHaveCount(0);

    releaseMe();
    await reload;

    await expect(page.locator('.skeleton-shell')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'User menu' })).toBeVisible();
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
  });

  test('handles expired session logout with 401 and still navigates to sign-in clearing local state', async ({
    page,
  }) => {
    await page.addInitScript(() =>
      localStorage.setItem(
        'app.tenant',
        JSON.stringify({
          id: 'tenant-1',
          name: 'Acme Support',
          slug: 'acme-support',
          status: 'active',
        }),
      ),
    );
    await page.route('**/api/v1/auth/logout', async (route) => {
      await route.fulfill({
        status: 401,
        contentType: 'application/json',
        body: JSON.stringify({ code: 'unauthenticated', message: 'Session expired', status: 401 }),
      });
    });
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await page.getByRole('button', { name: 'User menu' }).click();
    await page.locator('.sign-out').click();
    await expect(page).toHaveURL(/\/auth\/login$/);
    const tenantStorage = await page.evaluate(() => localStorage.getItem('app.tenant'));
    expect(tenantStorage).toBeNull();
  });

  test('shows loading state during delayed tenant switch then renders content without stale data', async ({
    page,
  }) => {
    test.slow();
    let releaseSwitch!: () => void;
    const switchReleased = new Promise<void>((resolve) => (releaseSwitch = resolve));

    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      if (path === '/me') return json(route, identities.platform);
      if (path === '/platform/tenants') return json(route, { items: [tenant], nextCursor: null });
      if (path === `/platform/tenants/${tenant.id}/switch`) {
        await switchReleased;
        return json(route, tenant);
      }
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/tenant/select');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeEmpty();

    await page.getByRole('button', { name: 'Switch tenant' }).click();
    await page.locator('.option').filter({ hasText: 'Acme Support' }).click();

    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeEmpty();

    releaseSwitch();

    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toContainText('Acme Support');
  });

  test('renders tenant switch without bootstrap skeleton', async ({ page }) => {
    test.slow();
    const tenants = [
      { id: 'tenant-a', name: 'Alpha Support', slug: 'alpha-support', status: 'active' },
      { id: 'tenant-b', name: 'Beta Support', slug: 'beta-support', status: 'active' },
    ];

    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      if (path === '/me') return json(route, identities.platform);
      if (path === '/platform/tenants') return json(route, { items: tenants, nextCursor: null });
      if (path === `/platform/tenants/${tenants[0].id}/switch`) return json(route, tenants[0]);
      if (path === `/platform/tenants/${tenants[1].id}/switch`) return json(route, tenants[1]);
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/tenant/select');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toBeEmpty();

    await page.getByRole('button', { name: 'Switch tenant' }).click();
    await page.locator('.option').filter({ hasText: 'Alpha Support' }).click();

    await expect(page.locator('.skeleton-shell')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Switch tenant' })).toContainText(
      'Alpha Support',
    );
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
  });

  test('applies theme-computed styles to all shell surfaces in light and dark mode', async ({
    page,
  }) => {
    test.slow();

    await page.addInitScript(() =>
      localStorage.setItem(
        'app.tenant',
        JSON.stringify({
          id: 'tenant-1',
          name: 'Acme Support',
          slug: 'acme-support',
          status: 'active',
        }),
      ),
    );
    await page.addInitScript(() => localStorage.setItem('app.themeMode', 'light'));

    // ── Light mode skeleton ──────────────────────────────────
    let releaseLight!: () => void;
    const lightIdReleased = new Promise<void>((resolve) => (releaseLight = resolve));
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      if (path === '/me') {
        await lightIdReleased;
        return json(route, identities.tenant);
      }
      if (path === '/platform/tenants') return json(route, { items: [tenant], nextCursor: null });
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null });
    });

    const navLight = page.goto('/tenant/overview');
    await expect(page.locator('.skeleton-shell')).toBeVisible();
    const skelLight = await page.evaluate(() => {
      const el = document.querySelector('.skeleton-shell');
      return el ? { bg: window.getComputedStyle(el).backgroundColor } : null;
    });
    expect(skelLight).not.toBeNull();
    releaseLight();
    await navLight;

    // ── Light mode surfaces ──────────────────────────────────
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
    const light = await page.evaluate(() => {
      const sidebar = document.querySelector('app-sidebar');
      const topbar = document.querySelector('app-topbar header');
      const breadcrumb = document.querySelector('nav[aria-label="Breadcrumb"]');
      const main = document.querySelector('main');
      return {
        sidebarBg: sidebar ? window.getComputedStyle(sidebar).backgroundColor : null,
        topbarBg: topbar ? window.getComputedStyle(topbar).backgroundColor : null,
        breadcrumbColor: breadcrumb ? window.getComputedStyle(breadcrumb).color : null,
        mainBg: main ? window.getComputedStyle(main).backgroundColor : null,
      };
    });
    expect(light.sidebarBg).toBeTruthy();
    expect(light.topbarBg).toBeTruthy();
    expect(light.breadcrumbColor).toBeTruthy();

    // ── Light mode dropdown ──────────────────────────────────
    await page.getByRole('button', { name: 'User menu' }).click();
    await expect(page.getByRole('menu')).toBeVisible();
    const lightDropdown = await page.evaluate(() => {
      const menu = document.querySelector('[role="menu"]');
      return menu ? window.getComputedStyle(menu).backgroundColor : null;
    });
    expect(lightDropdown).toBeTruthy();
    await page.keyboard.press('Escape');

    // ── Light mode page-header and page-container ────────────
    const lightPageHeader = await page.evaluate(() => {
      const el = document.querySelector('app-page-header h1');
      return el ? { color: window.getComputedStyle(el).color } : null;
    });
    expect(lightPageHeader).not.toBeNull();
    const lightPageContainer = await page.evaluate(() => {
      const el = document.querySelector('app-page-container');
      return el ? { bg: window.getComputedStyle(el).backgroundColor } : null;
    });
    expect(lightPageContainer).not.toBeNull();

    // ── Light mode empty state ───────────────────────────────
    await page.unrouteAll({ behavior: 'wait' });
    await mockApi(page, identities.noRole);
    await page.goto('/tenant/select');
    await expect(page.getByRole('heading', { name: 'No workspace access' })).toBeVisible();
    const emptyLight = await page.evaluate(() => {
      const heading = document.querySelector('h3, h2, h1');
      return heading ? { color: window.getComputedStyle(heading).color } : null;
    });
    expect(emptyLight).not.toBeNull();

    // ── Dark mode empty state ────────────────────────────────
    await page.unrouteAll({ behavior: 'wait' });
    await mockApi(page, identities.noRole);
    await page.addInitScript(() => localStorage.setItem('app.themeMode', 'dark'));
    await page.goto('/tenant/select');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await expect(page.getByRole('heading', { name: 'No workspace access' })).toBeVisible();
    const emptyDark = await page.evaluate(() => {
      const heading = document.querySelector('h3, h2, h1');
      return heading ? { color: window.getComputedStyle(heading).color } : null;
    });
    expect(emptyDark).not.toBeNull();
    expect(emptyDark!.color).not.toBe(emptyLight!.color);

    // ── Dark mode surfaces ───────────────────────────────────
    await page.unrouteAll({ behavior: 'wait' });
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Overview',
    );
    const dark = await page.evaluate(() => {
      const sidebar = document.querySelector('app-sidebar');
      const topbar = document.querySelector('app-topbar header');
      const breadcrumb = document.querySelector('nav[aria-label="Breadcrumb"]');
      const main = document.querySelector('main');
      return {
        sidebarBg: sidebar ? window.getComputedStyle(sidebar).backgroundColor : null,
        topbarBg: topbar ? window.getComputedStyle(topbar).backgroundColor : null,
        breadcrumbColor: breadcrumb ? window.getComputedStyle(breadcrumb).color : null,
        mainBg: main ? window.getComputedStyle(main).backgroundColor : null,
      };
    });
    expect(dark.sidebarBg).not.toBe(light.sidebarBg);
    expect(dark.topbarBg).not.toBe(light.topbarBg);
    expect(dark.breadcrumbColor).not.toBe(light.breadcrumbColor);
    expect(dark.sidebarBg).not.toBe('rgb(255, 255, 255)');
    expect(dark.sidebarBg).not.toBe('rgba(0, 0, 0, 0)');
    expect(dark.topbarBg).not.toBe('rgb(255, 255, 255)');
    expect(dark.topbarBg).not.toBe('rgba(0, 0, 0, 0)');

    // ── Dark mode dropdown ───────────────────────────────────
    await page.getByRole('button', { name: 'User menu' }).click();
    await expect(page.getByRole('menu')).toBeVisible();
    const darkDropdown = await page.evaluate(() => {
      const menu = document.querySelector('[role="menu"]');
      return menu ? window.getComputedStyle(menu).backgroundColor : null;
    });
    expect(darkDropdown).toBeTruthy();
    expect(darkDropdown).not.toBe(lightDropdown);
    await page.keyboard.press('Escape');

    // ── Dark mode page-header and page-container ─────────────
    const darkPageHeader = await page.evaluate(() => {
      const el = document.querySelector('app-page-header h1');
      return el ? { color: window.getComputedStyle(el).color } : null;
    });
    expect(darkPageHeader).not.toBeNull();
    expect(darkPageHeader!.color).not.toBe(lightPageHeader!.color);
    const darkPageContainer = await page.evaluate(() => {
      const el = document.querySelector('app-page-container');
      return el ? { bg: window.getComputedStyle(el).backgroundColor } : null;
    });
    expect(darkPageContainer).not.toBeNull();

    // ── Skeleton in dark mode ──────────────────────────────
    let releaseDark!: () => void;
    const darkIdReleased = new Promise<void>((resolve) => (releaseDark = resolve));
    await page.unrouteAll({ behavior: 'wait' });
    await page.route('**/api/v1/**', async (route) => {
      const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
      if (path === '/me') {
        await darkIdReleased;
        return json(route, identities.tenant);
      }
      if (path === '/platform/tenants') return json(route, { items: [tenant], nextCursor: null });
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null });
    });

    const navDark = page.goto('/tenant/overview');
    await expect(page.locator('.skeleton-shell')).toBeVisible();
    const skelDark = await page.evaluate(() => {
      const el = document.querySelector('.skeleton-shell');
      return el ? { bg: window.getComputedStyle(el).backgroundColor } : null;
    });
    expect(skelDark).not.toBeNull();
    expect(skelDark!.bg).not.toBe(skelLight!.bg);
    releaseDark();
    await navDark;
  });
});
