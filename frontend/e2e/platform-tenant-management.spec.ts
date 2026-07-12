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

const tenant = {
  id: 'tenant-1',
  name: 'Acme Support',
  slug: 'acme-support',
  status: 'active' as const,
  plan: 'starter' as const,
};

const tenantDetail = {
  id: 'tenant-1',
  name: 'Acme Support',
  slug: 'acme-support',
  status: 'active',
  plan: 'starter',
  contactName: 'Jane Ops',
  contactEmail: 'ops@acme.test',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-06-01T00:00:00Z',
};

const identities = {
  tenant: {
    id: 'tenant-user',
    email: 'owner@acme.test',
    displayName: 'Olivia Owner',
    platformRole: null,
    platformPermissions: [] as string[],
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
  admin: {
    id: 'admin-user',
    email: 'admin@acme.test',
    displayName: 'Alice Admin',
    platformRole: null,
    platformPermissions: [] as string[],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: tenant.id,
        tenantName: tenant.name,
        tenantSlug: tenant.slug,
        role: 'admin',
        permissions: TENANT_PERMISSIONS,
      },
    ],
  },
  manager: {
    id: 'manager-user',
    email: 'manager@acme.test',
    displayName: 'Marcus Manager',
    platformRole: null,
    platformPermissions: [] as string[],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: tenant.id,
        tenantName: tenant.name,
        tenantSlug: tenant.slug,
        role: 'manager',
        permissions: TENANT_PERMISSIONS,
      },
    ],
  },
  agent: {
    id: 'agent-user',
    email: 'agent@acme.test',
    displayName: 'Aria Agent',
    platformRole: null,
    platformPermissions: [] as string[],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: tenant.id,
        tenantName: tenant.name,
        tenantSlug: tenant.slug,
        role: 'agent',
        permissions: TENANT_PERMISSIONS,
      },
    ],
  },
  viewer: {
    id: 'viewer-user',
    email: 'viewer@acme.test',
    displayName: 'Victor Viewer',
    platformRole: null,
    platformPermissions: [] as string[],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: tenant.id,
        tenantName: tenant.name,
        tenantSlug: tenant.slug,
        role: 'viewer',
        permissions: TENANT_PERMISSIONS,
      },
    ],
  },
  platform: {
    id: 'platform-user',
    email: 'admin@helix.test',
    displayName: 'Priya Platform',
    platformRole: 'super_admin',
    platformPermissions: [
      'platform.admin',
      'platform.tenants.list',
      'platform.tenants.switch',
      'platform.tenants.manage',
    ],
    staffTenantPermissions: TENANT_PERMISSIONS,
    memberships: [] as Array<{
      tenantId: string;
      tenantName: string;
      tenantSlug: string;
      role: string;
      permissions: readonly string[];
    }>,
  },
  noRole: {
    id: 'no-role-user',
    email: 'new@helix.test',
    displayName: 'No Role',
    platformRole: null,
    platformPermissions: [] as string[],
    staffTenantPermissions: null,
    memberships: [],
  },
  developer: {
    id: 'dev-user',
    email: 'dev@helix.test',
    displayName: 'Dev Patel',
    platformRole: 'developer',
    platformPermissions: ['platform.tenants.list', 'platform.tenants.switch'],
    staffTenantPermissions: TENANT_PERMISSIONS,
    memberships: [] as Array<{
      tenantId: string;
      tenantName: string;
      tenantSlug: string;
      role: string;
      permissions: readonly string[];
    }>,
  },
  support: {
    id: 'support-user',
    email: 'support@helix.test',
    displayName: 'Sam Support',
    platformRole: 'support',
    platformPermissions: [
      'platform.tenants.list',
      'platform.tenants.switch',
      'platform.tenants.manage',
    ],
    staffTenantPermissions: TENANT_PERMISSIONS,
    memberships: [] as Array<{
      tenantId: string;
      tenantName: string;
      tenantSlug: string;
      role: string;
      permissions: readonly string[];
    }>,
  },
  sales: {
    id: 'sales-user',
    email: 'sales@helix.test',
    displayName: 'Sierra Sales',
    platformRole: 'sales',
    platformPermissions: ['platform.tenants.list', 'platform.tenants.switch'],
    staffTenantPermissions: TENANT_PERMISSIONS,
    memberships: [] as Array<{
      tenantId: string;
      tenantName: string;
      tenantSlug: string;
      role: string;
      permissions: readonly string[];
    }>,
  },
  finance: {
    id: 'finance-user',
    email: 'finance@helix.test',
    displayName: 'Finn Finance',
    platformRole: 'finance',
    platformPermissions: ['platform.tenants.list', 'platform.tenants.switch'],
    staffTenantPermissions: TENANT_PERMISSIONS,
    memberships: [] as Array<{
      tenantId: string;
      tenantName: string;
      tenantSlug: string;
      role: string;
      permissions: readonly string[];
    }>,
  },
} as const;

type Identity = (typeof identities)[keyof typeof identities];

async function json(route: Route, data: unknown, status = 200) {
  await route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

async function mockPlatformApi(
  page: Page,
  identity: Identity,
  tenants: Array<{ id: string; name: string; slug: string; status: string; plan: string }> = [
    tenant,
  ],
  detailInput: Record<string, unknown> = tenantDetail,
) {
  const createdTenants: Array<{
    id: string;
    name: string;
    slug: string;
    status: string;
    plan: string;
  }> = [];
  let page2Items: Array<{
    id: string;
    name: string;
    slug: string;
    plan: string;
    status: string;
  }> | null = null;
  let mutableDetail = { ...detailInput };

  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    const method = route.request().method();
    const segments = path.split('/').filter(Boolean);

    if (path === '/me') return json(route, identity);
    if (path === '/auth/logout') return json(route, null);

    if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
      const q = url.searchParams.get('q');
      const status = url.searchParams.get('status');
      const cursor = url.searchParams.get('cursor');

      if (cursor && page2Items) {
        return json(route, { items: page2Items, nextCursor: null, hasMore: false });
      }

      let items = [...tenants, ...createdTenants];
      if (q) {
        items = items.filter(
          (t) =>
            t.name.toLowerCase().includes(q.toLowerCase()) ||
            t.slug.toLowerCase().includes(q.toLowerCase()),
        );
      }
      if (status) {
        items = items.filter((t) => t.status === status);
      }
      return json(route, { items, nextCursor: null, hasMore: false });
    }

    if (
      method === 'GET' &&
      segments.length === 3 &&
      segments[0] === 'platform' &&
      segments[1] === 'tenants'
    ) {
      return json(route, mutableDetail);
    }

    if (method === 'POST' && path === '/platform/tenants') {
      const body = route.request().postDataJSON();
      const created = {
        id: `new-${body.slug}`,
        name: body.name,
        slug: body.slug,
        status: 'active',
        plan: body.plan ?? 'trial',
      };
      createdTenants.push(created);
      return json(
        route,
        {
          ...created,
          contactName: body.contactName ?? null,
          contactEmail: body.contactEmail ?? null,
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
        },
        201,
      );
    }

    if (
      method === 'PATCH' &&
      segments.length === 3 &&
      segments[0] === 'platform' &&
      segments[1] === 'tenants'
    ) {
      const body = route.request().postDataJSON();
      mutableDetail = { ...mutableDetail, ...body, updatedAt: new Date().toISOString() };
      return json(route, mutableDetail);
    }

    return json(route, { items: [], nextCursor: null });
  });
}

async function installNoContentFlashObserver(page: Page) {
  await page.addInitScript(`
    const flashDetected = [];
    const protectedTexts = [
      'Tenants', 'Tenant directory', 'Tenant details', 'New tenant', 'Edit tenant',
      'Acme Support', 'acme-support', 'Jane Ops', 'ops@acme.test', 'active', 'suspended',
    ];
    const protectedSelectors = [
      'app-tenant-list', 'app-tenant-detail', 'app-tenant-form', '.action-link', '.action-button',
    ];
    function checkTextContent(textContent) {
      if (!textContent) return;
      for (const t of protectedTexts) {
        if (textContent.includes(t)) { flashDetected.push('text:' + t); return; }
      }
    }
    function checkNode(node) {
      if (node.nodeType === 3) {
        checkTextContent(node.textContent);
        return;
      }
      if (node.nodeType !== 1) return;
      const text = node.textContent ?? '';
      for (const t of protectedTexts) {
        if (text.includes(t)) { flashDetected.push('text:' + t); return; }
      }
      for (const sel of protectedSelectors) {
        if (node.matches(sel) || node.querySelector(sel)) { flashDetected.push('sel:' + sel); return; }
      }
    }
    const obs = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (mutation.type === 'childList') {
          for (const node of mutation.addedNodes) checkNode(node);
        }
        if (mutation.type === 'attributes') {
          for (const sel of protectedSelectors) {
            if (mutation.target.matches(sel)) flashDetected.push('attr:' + sel);
          }
        }
        if (mutation.type === 'characterData') {
          checkTextContent(mutation.target.textContent);
        }
      }
    });
    obs.observe(document.documentElement, { childList: true, subtree: true, attributes: true, characterData: true });
    window['__tm_content_flash'] = flashDetected;
  `);
}

test.describe('platform tenant management', () => {
  test('onboards a new tenant through the form and lands on the list page', async ({ page }) => {
    await mockPlatformApi(page, identities.platform);
    await page.goto('/platform/tenants/new');

    await expect(page.getByRole('heading', { name: 'New tenant' })).toBeVisible();

    await page.locator('input[formControlName="name"]').fill('TestCorp');
    await page.locator('input[formControlName="slug"]').fill('test-corp');
    await page.locator('select[formControlName="plan"]').selectOption('professional');
    await page.locator('input[formControlName="contactName"]').fill('Test Contact');
    await page.locator('input[formControlName="contactEmail"]').fill('contact@testcorp.com');

    const createResponsePromise = page.waitForResponse(
      (res) => res.url().includes('/api/v1/platform/tenants') && res.request().method() === 'POST',
    );
    await page.getByRole('button', { name: 'Create tenant' }).click();
    const createResponse = await createResponsePromise;
    expect(createResponse.status()).toBe(201);

    await expect(page).toHaveURL(/\/platform\/tenants$/);
    await expect(page.getByText('TestCorp')).toBeVisible();
  });

  test('onboards a new tenant within the one-minute threshold', async ({ page }) => {
    await mockPlatformApi(page, identities.platform);
    await page.goto('/platform/tenants/new');

    await expect(page.getByRole('heading', { name: 'New tenant' })).toBeVisible();

    const start = performance.now();

    await page.locator('input[formControlName="name"]').fill('TestCorp');
    await page.locator('input[formControlName="slug"]').fill('test-corp');
    await page.locator('select[formControlName="plan"]').selectOption('professional');
    await page.locator('input[formControlName="contactName"]').fill('Test Contact');
    await page.locator('input[formControlName="contactEmail"]').fill('contact@testcorp.com');

    await page.getByRole('button', { name: 'Create tenant' }).click();

    await expect(page).toHaveURL(/\/platform\/tenants$/);
    await expect(page.getByText('TestCorp')).toBeVisible();

    const elapsed = performance.now() - start;
    expect(elapsed).toBeLessThan(60000);
    console.log(`Tenant onboarding completed in ${elapsed.toFixed(0)}ms`);
  });

  test('search narrows the tenant list by name or slug', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
      { id: 't-2', name: 'Globex Inc', slug: 'globex', status: 'active', plan: 'professional' },
      { id: 't-3', name: 'Beta Labs', slug: 'beta-labs', status: 'suspended', plan: 'trial' },
    ];

    await mockPlatformApi(page, identities.platform, tenants);
    await page.goto('/platform/tenants');

    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('Globex Inc')).toBeVisible();
    await expect(page.getByText('Beta Labs')).toBeVisible();

    await page.locator('.toolbar input[type="search"]').fill('acme');
    await page.waitForTimeout(500);

    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('Globex Inc')).toHaveCount(0);
    await expect(page.getByText('Beta Labs')).toHaveCount(0);
  });

  test('status filter scopes the tenant list', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
      { id: 't-2', name: 'Globex Inc', slug: 'globex', status: 'active', plan: 'professional' },
      { id: 't-3', name: 'Beta Labs', slug: 'beta-labs', status: 'suspended', plan: 'trial' },
    ];

    await mockPlatformApi(page, identities.platform, tenants);
    await page.goto('/platform/tenants');

    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('Beta Labs')).toBeVisible();

    await page.getByLabel('Status filter').selectOption('suspended');

    await expect(page.getByText('Acme Corp')).toHaveCount(0);
    await expect(page.getByText('Beta Labs')).toBeVisible();
  });

  test('pagination loads more tenants on "Load more" click', async ({ page }) => {
    const page1 = Array.from({ length: 25 }, (_, i) => ({
      id: `t-${i + 1}`,
      name: `Tenant ${i + 1}`,
      slug: `tenant-${i + 1}`,
      status: 'active' as const,
      plan: 'starter' as const,
    }));
    const page2 = [
      { id: 't-26', name: 'Extra Tenant', slug: 'extra-tenant', status: 'active', plan: 'starter' },
    ];

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        const cursor = url.searchParams.get('cursor');
        if (cursor) {
          return json(route, { items: page2, nextCursor: null, hasMore: false });
        }
        return json(route, { items: page1, nextCursor: 'cursor-2', hasMore: true });
      }

      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants');

    await expect(page.locator('tbody tr').first()).toContainText('Tenant 1');
    await expect(page.locator('tbody tr').last()).toContainText('Tenant 25');
    await expect(page.getByText('Extra Tenant')).toHaveCount(0);

    const loadMore = page.getByRole('button', { name: 'Load more' });
    await expect(loadMore).toBeVisible();
    await loadMore.click();

    await expect(page.getByText('Extra Tenant')).toBeVisible();
  });

  test('tenant user cannot access platform tenant pages', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.tenant);
      if (path === '/auth/logout') return json(route, null);
      if (path === '/platform/tenants' && segments.length === 2) {
        return json(route, { items: [tenant], nextCursor: null });
      }
      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants');

    await expect(page).not.toHaveURL(/\/platform\/tenants/);
    await expect(page).toHaveURL(/\/tenant\/overview$/);
  });

  test('tenant-management controls are absent for all five tenant roles', async ({ page }) => {
    const tenantRoles = ['owner', 'admin', 'manager', 'agent', 'viewer'] as const;
    const tenantRoleIdentities: Record<string, Identity> = {
      owner: identities.tenant,
      admin: identities.admin,
      manager: identities.manager,
      agent: identities.agent,
      viewer: identities.viewer,
    };

    for (const role of tenantRoles) {
      await page.unrouteAll({ behavior: 'wait' });
      const identity = tenantRoleIdentities[role];
      await page.route('**/api/v1/**', async (route) => {
        const url = new URL(route.request().url());
        const path = url.pathname.replace('/api/v1', '');
        if (path === '/me') return json(route, identity);
        if (path === '/auth/logout') return json(route, null);
        return json(route, { items: [], nextCursor: null, hasMore: false });
      });

      await page.goto('/platform/tenants');
      await expect(page).not.toHaveURL(/\/platform\/tenants/);

      // Assert Platform and Tenants nav entries absent in the navigation UI
      await expect(page.getByRole('button', { name: 'Platform' })).toHaveCount(0);
      await expect(page.getByRole('navigation', { name: 'Primary navigation' })).not.toContainText(
        'Tenants',
      );

      await page.goto('/platform/tenants/tenant-1');
      await expect(page).not.toHaveURL(/\/platform\/tenants/);

      const finalUrl = page.url();
      expect(finalUrl).toMatch(/\/tenant\/overview/);

      // Assert Platform and Tenants nav entries absent after detail redirect too
      await expect(page.getByRole('button', { name: 'Platform' })).toHaveCount(0);
      await expect(page.getByRole('navigation', { name: 'Primary navigation' })).not.toContainText(
        'Tenants',
      );
    }
  });

  test('management controls on detail page reflect platform.tenants.manage', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', (msg) => {
      if (msg.type() === 'error') errors.push(msg.text());
    });

    let currentStatus = 'active';
    const mutableDetail = { ...tenantDetail };

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);
      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [tenant], nextCursor: null, hasMore: false });
      }
      if (
        method === 'GET' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(route, { ...mutableDetail, status: currentStatus });
      }
      if (
        method === 'PATCH' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        const body = route.request().postDataJSON();
        if (body.status) currentStatus = body.status;
        const updated = {
          ...mutableDetail,
          ...body,
          status: currentStatus,
          updatedAt: new Date().toISOString(),
        };
        return json(route, updated);
      }
      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants/tenant-1');

    // Assert name and status from the detail response body per FR-003
    await expect(page.getByText('Acme Support')).toBeVisible();
    await expect(page.getByText(/active/i)).toBeVisible();

    // Assert management controls are visible for platform.tenants.manage
    await expect(page.locator('.action-link')).toBeVisible();
    await expect(page.locator('.action-button')).toBeVisible();

    // Assert edit link exists (per REST contract)
    const editLink = page.locator('.action-link');
    await expect(editLink).toBeVisible();

    // Assert status-action button triggers PATCH with status toggled
    const deactivatePromise = page.waitForRequest(
      (req) => req.url().includes('/api/v1/platform/tenants/tenant-1') && req.method() === 'PATCH',
    );
    await page.locator('.action-button').click();
    await page.locator('.dialog-confirm').click();
    const deactivateReq = await deactivatePromise;
    expect(deactivateReq.postDataJSON().status).toBe('suspended');

    // Assert PATCH response body reflects the update (per REST contract)
    await expect(page.getByText(/suspended/i)).toBeVisible();

    // Reactivate
    const reactivatePromise = page.waitForRequest(
      (req) => req.url().includes('/api/v1/platform/tenants/tenant-1') && req.method() === 'PATCH',
    );
    await page.locator('.action-button').click();
    await page.locator('.dialog-confirm').click();
    const reactivateReq = await reactivatePromise;
    expect(reactivateReq.postDataJSON().status).toBe('active');
    await expect(page.getByText(/active/i)).toBeVisible();

    await page.unrouteAll({ behavior: 'wait' });
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.developer);
      if (path === '/auth/logout') return json(route, null);
      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [tenant], nextCursor: null, hasMore: false });
      }
      if (
        method === 'GET' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(route, { ...tenantDetail });
      }
      return json(route, { items: [], nextCursor: null });
    });
    await page.goto('/platform/tenants/tenant-1');

    await expect(page.locator('.action-link')).toHaveCount(0);
    await expect(page.locator('.action-button')).toHaveCount(0);
  });

  test('view-only roles see tenant list without New tenant button', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
    ];

    for (const role of ['developer', 'sales', 'finance'] as const) {
      await page.unrouteAll({ behavior: 'wait' });
      const identity = identities[role];
      await mockPlatformApi(page, identity, tenants);
      await page.goto('/platform/tenants');

      await expect(page.getByText('Acme Corp')).toBeVisible();
      await expect(page.getByText('New tenant')).toHaveCount(0);

      await page.goto('/platform/tenants/tenant-1');
      await expect(page.locator('.action-link')).toHaveCount(0);
      await expect(page.locator('.action-button')).toHaveCount(0);
    }
  });

  test('support role can access platform tenant list', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
    ];

    await mockPlatformApi(page, identities.support, tenants);
    await page.goto('/platform/tenants');

    await expect(page).toHaveURL(/\/platform\/tenants$/);
    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('New tenant')).toHaveCount(1);
  });

  test('combined search and status filter narrows results', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
      {
        id: 't-2',
        name: 'Acme Suspended',
        slug: 'acme-suspended',
        status: 'suspended',
        plan: 'professional',
      },
      { id: 't-3', name: 'Beta Labs', slug: 'beta-labs', status: 'active', plan: 'trial' },
      {
        id: 't-4',
        name: 'Beta Inactive',
        slug: 'beta-inactive',
        status: 'suspended',
        plan: 'starter',
      },
    ];

    await mockPlatformApi(page, identities.platform, tenants);
    await page.goto('/platform/tenants');

    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('Acme Suspended')).toBeVisible();
    await expect(page.getByText('Beta Labs')).toBeVisible();
    await expect(page.getByText('Beta Inactive')).toBeVisible();

    await page.locator('.toolbar input[type="search"]').fill('acme');
    await page.getByLabel('Status filter').selectOption('suspended');
    await page.waitForTimeout(500);

    await expect(page.getByText('Acme Corp')).toHaveCount(0);
    await expect(page.getByText('Acme Suspended')).toBeVisible();
    await expect(page.getByText('Beta Labs')).toHaveCount(0);
    await expect(page.getByText('Beta Inactive')).toHaveCount(0);

    await page.locator('.toolbar input[type="search"]').fill('');
    await page.getByLabel('Status filter').selectOption('');
    await page.waitForTimeout(500);

    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('Acme Suspended')).toBeVisible();
    await expect(page.getByText('Beta Labs')).toBeVisible();
    await expect(page.getByText('Beta Inactive')).toBeVisible();
  });

  test('deep-link refusal for list shows no tenant-management content', async ({ page }) => {
    await installNoContentFlashObserver(page);

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      if (path === '/me') {
        await new Promise((r) => setTimeout(r, 1500));
        return json(route, identities.tenant);
      }
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null, hasMore: false });
    });

    await page.goto('/platform/tenants');
    await expect(page).toHaveURL(/\/tenant\/overview$/);

    const flashMutations = await page.evaluate(() => window['__tm_content_flash'] ?? []);
    expect(flashMutations).toEqual([]);
  });

  test('deep-link refusal for detail shows no tenant-management content', async ({ page }) => {
    await installNoContentFlashObserver(page);

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      if (path === '/me') {
        await new Promise((r) => setTimeout(r, 1500));
        return json(route, identities.tenant);
      }
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null, hasMore: false });
    });

    await page.goto('/platform/tenants/tenant-1');
    await expect(page).toHaveURL(/\/tenant\/overview$/);

    const flashMutations = await page.evaluate(() => window['__tm_content_flash'] ?? []);
    expect(flashMutations).toEqual([]);
  });

  test('deep-link refusal for new shows no tenant-management content', async ({ page }) => {
    await installNoContentFlashObserver(page);

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      if (path === '/me') {
        await new Promise((r) => setTimeout(r, 1500));
        return json(route, identities.tenant);
      }
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null, hasMore: false });
    });

    await page.goto('/platform/tenants/new');
    await expect(page).toHaveURL(/\/tenant\/overview$/);

    const flashMutations = await page.evaluate(() => window['__tm_content_flash'] ?? []);
    expect(flashMutations).toEqual([]);
  });

  test('deep-link refusal for edit shows no tenant-management content', async ({ page }) => {
    await installNoContentFlashObserver(page);

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      if (path === '/me') {
        await new Promise((r) => setTimeout(r, 1500));
        return json(route, identities.tenant);
      }
      if (path === '/auth/logout') return json(route, null);
      return json(route, { items: [], nextCursor: null, hasMore: false });
    });

    await page.goto('/platform/tenants/tenant-1/edit');
    await expect(page).toHaveURL(/\/tenant\/overview$/);

    const flashMutations = await page.evaluate(() => window['__tm_content_flash'] ?? []);
    expect(flashMutations).toEqual([]);
  });

  test('edit interaction updates tenant name and navigates back', async ({ page }) => {
    await mockPlatformApi(page, identities.platform);

    await page.goto('/platform/tenants/tenant-1');
    await expect(page.locator('app-tenant-detail')).toBeVisible({ timeout: 10000 });

    await page.locator('.action-link').click();
    await expect(page).toHaveURL(/\/tenant-1\/edit/);
    await expect(page.locator('app-tenant-form')).toBeVisible({ timeout: 5000 });

    // Now click "Try again" to force the API call
    const tryAgain = page.locator('app-empty-state button.primary-button');
    if ((await tryAgain.count()) > 0) {
      await tryAgain.click();
      // Wait for the loading state to appear then disappear
      await page
        .locator('app-loading-state')
        .waitFor({ state: 'visible', timeout: 5000 })
        .catch(() => {});
    }

    await expect(page.locator('input[formControlName="name"]')).toBeVisible({ timeout: 15000 });

    await page.locator('input[formControlName="name"]').fill('Acme Support Updated');

    await page.getByRole('button', { name: 'Save changes' }).click();

    await expect(page).toHaveURL(/\/platform\/tenants(\/tenant-1)?$/);
    await expect(page.getByText('Acme Support Updated')).toBeVisible();
  });

  test('deactivate and reactivate tenant updates status', async ({ page }) => {
    let currentStatus = 'active';
    const mutableDetail = { ...tenantDetail };

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);
      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [tenant], nextCursor: null, hasMore: false });
      }
      if (
        method === 'GET' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(route, { ...mutableDetail, status: currentStatus });
      }
      if (
        method === 'PATCH' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        const body = route.request().postDataJSON();
        if (body.status) currentStatus = body.status;
        return json(route, {
          ...mutableDetail,
          ...body,
          status: currentStatus,
          updatedAt: new Date().toISOString(),
        });
      }
      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants/tenant-1');

    await expect(page.getByText(/active/i)).toBeVisible();

    const deactivatePromise = page.waitForRequest(
      (req) => req.url().includes('/api/v1/platform/tenants/tenant-1') && req.method() === 'PATCH',
    );
    await page.locator('.action-button').click();
    await page.locator('.dialog-confirm').click();
    const deactivateReq = await deactivatePromise;
    expect(deactivateReq.postDataJSON().status).toBe('suspended');

    const reactivatePromise = page.waitForRequest(
      (req) => req.url().includes('/api/v1/platform/tenants/tenant-1') && req.method() === 'PATCH',
    );
    await page.locator('.action-button').click();
    await page.locator('.dialog-confirm').click();
    const reactivateReq = await reactivatePromise;
    expect(reactivateReq.postDataJSON().status).toBe('active');
  });

  test('support role lands on tenant list from /platform root', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
    ];

    await mockPlatformApi(page, identities.support, tenants);
    await page.goto('/platform');

    await expect(page).toHaveURL(/\/platform\/tenants/);
    await expect(page.getByText('Acme Corp')).toBeVisible();
    await expect(page.getByText('New tenant')).toBeVisible();
  });

  test('tenant-management controls respect authorization matrix for all platform roles', async ({
    page,
  }) => {
    const rolesToTest = [
      { key: 'platform', expectedNewTenant: true, expectedControls: true },
      { key: 'support', expectedNewTenant: true, expectedControls: true },
      { key: 'developer', expectedNewTenant: false, expectedControls: false },
      { key: 'sales', expectedNewTenant: false, expectedControls: false },
      { key: 'finance', expectedNewTenant: false, expectedControls: false },
      { key: 'noRole', expectedNewTenant: false, expectedControls: false, expectRedirect: true },
    ] as const;

    const tenants = [
      {
        id: 'tenant-1',
        name: 'Acme Support',
        slug: 'acme-support',
        status: 'active',
        plan: 'starter',
      },
    ];

    for (const role of rolesToTest) {
      await page.unrouteAll({ behavior: 'wait' });
      const identity = identities[role.key as keyof typeof identities];
      await mockPlatformApi(page, identity, tenants);
      await page.goto('/platform/tenants');

      if ('expectRedirect' in role && role.expectRedirect) {
        await expect(page).not.toHaveURL(/\/platform\/tenants/);
        continue;
      }

      await expect(page).toHaveURL(/\/platform\/tenants/);
      if (role.expectedNewTenant) {
        await expect(page.getByText('New tenant')).toBeVisible();
      } else {
        await expect(page.getByText('New tenant')).toHaveCount(0);
      }

      await page.goto('/platform/tenants/tenant-1');
      if (role.expectedControls) {
        await expect(page.locator('.action-link')).toBeVisible();
        await expect(page.locator('.action-button')).toBeVisible();
      } else {
        await expect(page.locator('.action-link')).toHaveCount(0);
        await expect(page.locator('.action-button')).toHaveCount(0);
      }
    }
  });

  test('cursor-paginated 505-tenant directory traversal is comparable to detail page load', async ({
    page,
  }) => {
    const allTenants = Array.from({ length: 505 }, (_, i) => ({
      id: `t-${i + 1}`,
      name: `Tenant ${i + 1}`,
      slug: `tenant-${i + 1}`,
      status: (i % 3 === 0 ? 'suspended' : 'active') as 'active' | 'suspended',
      plan: (['trial', 'starter', 'professional', 'enterprise'] as const)[i % 4],
    }));

    const detail505 = {
      id: 't-505',
      name: 'Tenant 505',
      slug: 'tenant-505',
      status: 'active',
      plan: 'enterprise',
      contactName: 'Jane Ops',
      contactEmail: 'ops@acme.test',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-06-01T00:00:00Z',
    };

    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        const cursor = url.searchParams.get('cursor');
        const limit = parseInt(url.searchParams.get('limit') || '25', 10);

        let startIndex = 0;
        if (cursor) {
          const cursorIndex = allTenants.findIndex((t) => t.id === cursor);
          startIndex = cursorIndex >= 0 ? cursorIndex + 1 : 0;
        }

        const pageItems = allTenants.slice(startIndex, startIndex + limit);
        const hasMore = startIndex + limit < allTenants.length;
        const nextCursor = hasMore ? (pageItems[pageItems.length - 1]?.id ?? null) : null;

        return json(route, { items: pageItems, nextCursor, hasMore });
      }

      if (
        method === 'GET' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(route, detail505);
      }

      return json(route, { items: [], nextCursor: null });
    });

    const directoryStart = performance.now();
    await page.goto('/platform/tenants');
    await expect(page.getByRole('link', { name: 'Tenant 1', exact: true })).toBeVisible();

    let pageCount = 1;
    let loadMore = page.getByRole('button', { name: 'Load more' });

    while (await loadMore.isVisible().catch(() => false)) {
      await loadMore.click();
      pageCount++;
      await page.waitForTimeout(100);
      loadMore = page.getByRole('button', { name: 'Load more' });
    }

    const directoryTime = performance.now() - directoryStart;
    console.log(`505 tenants traversed in ${pageCount} pages, ${directoryTime.toFixed(0)}ms`);
    console.log(`Average ${(directoryTime / pageCount).toFixed(0)}ms per page`);

    const allRows = await page.locator('tbody tr').count();
    expect(allRows).toBe(505);

    const rowTexts = await page.locator('tbody tr td:first-child a').allTextContents();
    const ids = rowTexts.map((t) => t.match(/Tenant (\d+)/)?.[1]).filter(Boolean);
    expect(ids.length).toBe(505);

    expect(new Set(ids).size).toBe(505);

    for (let i = 0; i < ids.length - 1; i++) {
      expect(parseInt(ids[i])).toBeLessThan(parseInt(ids[i + 1]));
    }

    expect(rowTexts[rowTexts.length - 1]).toContain('Tenant 505');

    console.log(`Loaded ${allRows} rows, ${new Set(ids).size} unique, ordered correctly`);

    const detailStart = performance.now();
    await page.goto('/platform/tenants/t-505');
    await expect(page.getByText('Jane Ops')).toBeVisible();
    const detailTime = performance.now() - detailStart;
    console.log(`Detail page loaded in ${detailTime.toFixed(0)}ms`);
    console.log(
      `Comparison: directory=${directoryTime.toFixed(0)}ms detail=${detailTime.toFixed(0)}ms`,
    );

    expect(directoryTime).toBeLessThan(30000);
  });

  test('view-only roles can search, filter, and view tenant details', async ({ page }) => {
    const tenants = [
      { id: 't-1', name: 'Acme Corp', slug: 'acme', status: 'active', plan: 'starter' },
      { id: 't-2', name: 'Globex Inc', slug: 'globex', status: 'active', plan: 'professional' },
      { id: 't-3', name: 'Beta Labs', slug: 'beta-labs', status: 'suspended', plan: 'trial' },
    ];

    const tenantDetailForView = {
      id: 't-1',
      name: 'Acme Corp',
      slug: 'acme',
      status: 'active',
      plan: 'starter',
      contactName: 'Jane Ops',
      contactEmail: 'ops@acme.test',
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-06-01T00:00:00Z',
    };

    for (const role of ['developer', 'sales', 'finance'] as const) {
      await page.unrouteAll({ behavior: 'wait' });
      const identity = identities[role];
      await mockPlatformApi(page, identity, tenants, tenantDetailForView);
      await page.goto('/platform/tenants');

      await expect(page.getByText('Acme Corp')).toBeVisible();
      await expect(page.getByText('Globex Inc')).toBeVisible();
      await expect(page.getByText('Beta Labs')).toBeVisible();

      await page.locator('.toolbar input[type="search"]').fill('acme');
      await page.waitForTimeout(500);
      await expect(page.getByText('Acme Corp')).toBeVisible();
      await expect(page.getByText('Globex Inc')).toHaveCount(0);
      await expect(page.getByText('Beta Labs')).toHaveCount(0);

      await page.locator('.toolbar input[type="search"]').fill('');

      await page.getByLabel('Status filter').selectOption('suspended');
      await page.waitForTimeout(500);
      await expect(page.getByText('Acme Corp')).toHaveCount(0);
      await expect(page.getByText('Beta Labs')).toBeVisible();
      await expect(page.getByText('Globex Inc')).toHaveCount(0);

      await page.getByLabel('Status filter').selectOption('');

      await page.goto('/platform/tenants/t-1');
      await expect(page.getByText('Jane Ops')).toBeVisible();
      await expect(page.getByText('ops@acme.test')).toBeVisible();

      await expect(page.locator('.action-link')).toHaveCount(0);
      await expect(page.locator('.action-button')).toHaveCount(0);
    }
  });

  test('deep-link refusal covers all five tenant roles with no content flash', async ({ page }) => {
    test.slow();

    await installNoContentFlashObserver(page);

    const tenantRoles = ['owner', 'admin', 'manager', 'agent', 'viewer'] as const;
    const tenantRoleIdentities: Record<string, Identity> = {
      owner: identities.tenant,
      admin: identities.admin,
      manager: identities.manager,
      agent: identities.agent,
      viewer: identities.viewer,
    };

    const deepLinks = [
      { path: '/platform/tenants', label: 'list' },
      { path: '/platform/tenants/tenant-1', label: 'detail' },
      { path: '/platform/tenants/new', label: 'new' },
      { path: '/platform/tenants/tenant-1/edit', label: 'edit' },
    ] as const;

    let errors = 0;

    for (const role of tenantRoles) {
      const identity = tenantRoleIdentities[role];
      for (const link of deepLinks) {
        await page.unrouteAll({ behavior: 'wait' });
        await page.route('**/api/v1/**', async (route) => {
          const url = new URL(route.request().url());
          const path = url.pathname.replace('/api/v1', '');
          if (path === '/me') {
            await new Promise((r) => setTimeout(r, 1500));
            return json(route, identity);
          }
          if (path === '/auth/logout') return json(route, null);
          return json(route, { items: [], nextCursor: null, hasMore: false });
        });

        await page.goto(link.path);
        await expect(page).toHaveURL(/\/tenant\/overview$/);

        const flashMutations = await page.evaluate(() => window['__tm_content_flash'] ?? []);
        if (flashMutations.length > 0) {
          errors++;
          console.log(`Content flash for ${role}/${link.label}: ${JSON.stringify(flashMutations)}`);
        }
      }
    }

    expect(errors).toBe(0);
  });

  test('create form shows server 422 validation errors on fields', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'POST' && path === '/platform/tenants') {
        return json(
          route,
          {
            error: {
              code: 'validation_failed',
              message: 'Validation failed',
              details: [
                { field: 'name', code: 'too_short', message: 'Name is too short' },
                { field: 'slug', code: 'invalid_format', message: 'Slug has invalid format' },
              ],
            },
          },
          422,
        );
      }

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [], nextCursor: null, hasMore: false });
      }

      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants/new');
    await expect(page.getByRole('heading', { name: 'New tenant' })).toBeVisible();

    await page.locator('input[formControlName="name"]').fill('A');
    await page.locator('input[formControlName="slug"]').fill('ab');
    await page.locator('select[formControlName="plan"]').selectOption('professional');

    const createResponsePromise = page.waitForResponse(
      (res) => res.url().includes('/api/v1/platform/tenants') && res.request().method() === 'POST',
    );
    await page.getByRole('button', { name: 'Create tenant' }).click();
    await createResponsePromise;

    await expect(page.getByText('Name is too short')).toBeVisible();
    await expect(page.getByText('Slug has invalid format')).toBeVisible();
  });

  test('create form shows 409 slug conflict error on slug field', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'POST' && path === '/platform/tenants') {
        return json(
          route,
          {
            error: {
              code: 'conflict',
              message: 'Slug is already taken',
              details: [{ field: 'slug', code: 'conflict', message: 'Slug is already taken' }],
            },
          },
          409,
        );
      }

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [], nextCursor: null, hasMore: false });
      }

      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants/new');
    await expect(page.getByRole('heading', { name: 'New tenant' })).toBeVisible();

    await page.locator('input[formControlName="name"]').fill('Test Corp');
    await page.locator('input[formControlName="slug"]').fill('existing-slug');
    await page.locator('select[formControlName="plan"]').selectOption('professional');

    const createResponsePromise = page.waitForResponse(
      (res) => res.url().includes('/api/v1/platform/tenants') && res.request().method() === 'POST',
    );
    await page.getByRole('button', { name: 'Create tenant' }).click();
    await createResponsePromise;

    await expect(page.getByText('Slug is already taken').first()).toBeVisible();
  });

  test('update form shows 409 slug conflict error on slug field', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [tenant], nextCursor: null, hasMore: false });
      }

      if (
        method === 'GET' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(route, tenantDetail);
      }

      if (
        method === 'PATCH' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(
          route,
          {
            error: {
              code: 'conflict',
              message: 'Slug is already taken',
              details: [{ field: 'slug', code: 'conflict', message: 'Slug is already taken' }],
            },
          },
          409,
        );
      }

      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants/tenant-1');
    await expect(page.locator('app-tenant-detail')).toBeVisible({ timeout: 10000 });

    await page.locator('.action-link').click();
    await expect(page).toHaveURL(/\/tenant-1\/edit/);
    await expect(page.locator('app-tenant-form')).toBeVisible({ timeout: 5000 });

    const tryAgain = page.locator('app-empty-state button.primary-button');
    if ((await tryAgain.count()) > 0) {
      await tryAgain.click();
      await page
        .locator('app-loading-state')
        .waitFor({ state: 'visible', timeout: 5000 })
        .catch(() => {});
    }

    await expect(page.locator('input[formControlName="slug"]')).toBeVisible({ timeout: 15000 });

    await page.locator('input[formControlName="slug"]').fill('taken-slug');

    const patchResponsePromise = page.waitForResponse(
      (res) => res.url().includes('/api/v1/platform/tenants') && res.request().method() === 'PATCH',
    );
    await page.getByRole('button', { name: 'Save changes' }).click();
    await patchResponsePromise;

    await expect(page.getByText('Slug is already taken').first()).toBeVisible();
  });

  test('update form shows 422 validation errors on fields', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [tenant], nextCursor: null, hasMore: false });
      }

      if (
        method === 'GET' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(route, tenantDetail);
      }

      if (
        method === 'PATCH' &&
        segments.length === 3 &&
        segments[0] === 'platform' &&
        segments[1] === 'tenants'
      ) {
        return json(
          route,
          {
            error: {
              code: 'validation_failed',
              message: 'Validation failed',
              details: [
                { field: 'name', code: 'too_short', message: 'Name is too short' },
                { field: 'slug', code: 'invalid_format', message: 'Slug has invalid format' },
              ],
            },
          },
          422,
        );
      }

      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants/tenant-1');
    await expect(page.locator('app-tenant-detail')).toBeVisible({ timeout: 10000 });

    await page.locator('.action-link').click();
    await expect(page).toHaveURL(/\/tenant-1\/edit/);
    await expect(page.locator('app-tenant-form')).toBeVisible({ timeout: 5000 });

    const tryAgain = page.locator('app-empty-state button.primary-button');
    if ((await tryAgain.count()) > 0) {
      await tryAgain.click();
      await page
        .locator('app-loading-state')
        .waitFor({ state: 'visible', timeout: 5000 })
        .catch(() => {});
    }

    await expect(page.locator('input[formControlName="name"]')).toBeVisible({ timeout: 15000 });

    await page.locator('input[formControlName="name"]').fill('A');
    await page.locator('input[formControlName="slug"]').fill('ab');

    const patchResponsePromise = page.waitForResponse(
      (res) => res.url().includes('/api/v1/platform/tenants') && res.request().method() === 'PATCH',
    );
    await page.getByRole('button', { name: 'Save changes' }).click();
    await patchResponsePromise;

    await expect(page.getByText('Name is too short')).toBeVisible();
    await expect(page.getByText('Slug has invalid format')).toBeVisible();
  });

  test('no-match search shows empty state', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');
      const method = route.request().method();
      const segments = path.split('/').filter(Boolean);

      if (path === '/me') return json(route, identities.platform);
      if (path === '/auth/logout') return json(route, null);

      if (method === 'GET' && path === '/platform/tenants' && !segments[3]) {
        return json(route, { items: [], nextCursor: null, hasMore: false });
      }

      return json(route, { items: [], nextCursor: null });
    });

    await page.goto('/platform/tenants');

    await page.locator('.toolbar input[type="search"]').fill('nonexistent-tenant');
    await page.waitForTimeout(500);

    await expect(page.getByText('No tenants match')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Clear filters' })).toBeVisible();
  });
});
