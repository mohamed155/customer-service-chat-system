import { expect, Page, Route, test } from '@playwright/test';

type CustomerChannel = 'email' | 'phone' | 'web_chat' | 'whatsapp' | 'telegram';

type CustomerListItem = {
  id: string;
  display_name: string;
  email: string | null;
  phone: string | null;
  channels: CustomerChannel[];
  created_at: string;
  updated_at: string;
};

type CustomerIdentifier = {
  id: string;
  channel: CustomerChannel;
  identifier: string;
};

type CustomerDetail = CustomerListItem & {
  identifiers: CustomerIdentifier[];
  metadata: Record<string, string>;
};

type ConversationStatus = 'open' | 'pending' | 'resolved' | 'closed';

type ConversationSummary = {
  id: string;
  channel: CustomerChannel;
  status: ConversationStatus;
  last_activity_at: string;
  created_at: string;
};

type PaginatedResponse<T> = {
  data: T[];
  pagination: {
    next_cursor: string | null;
    has_more: boolean;
  };
};

type ApiResponse<T> = {
  data: T;
};

type MembershipSummary = {
  tenantId: string;
  tenantName: string;
  tenantSlug: string;
  role: string;
  permissions: string[];
};

type CurrentUser = {
  id: string;
  email: string;
  displayName: string;
  platformRole: string | null;
  platformPermissions: string[];
  staffTenantPermissions: string[] | null;
  memberships: MembershipSummary[];
};

const CUSTOMER_MANAGE_PERMISSIONS = ['customers.view', 'customers.manage'] as const;
const CUSTOMER_VIEW_PERMISSIONS = ['customers.view'] as const;

const customerTenant = {
  id: 'tenant-customers',
  name: 'Customer Co',
  slug: 'customer-co',
  status: 'active' as const,
  plan: 'starter' as const,
};

function makeManagerIdentity(): CurrentUser {
  return {
    id: 'user-manager',
    email: 'manager@customer.test',
    displayName: 'Maria Manager',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: customerTenant.id,
        tenantName: customerTenant.name,
        tenantSlug: customerTenant.slug,
        role: 'manager',
        permissions: [...CUSTOMER_MANAGE_PERMISSIONS],
      },
    ],
  };
}

function makeViewerIdentity(): CurrentUser {
  return {
    id: 'user-viewer',
    email: 'viewer@customer.test',
    displayName: 'Victor Viewer',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: customerTenant.id,
        tenantName: customerTenant.name,
        tenantSlug: customerTenant.slug,
        role: 'viewer',
        permissions: [...CUSTOMER_VIEW_PERMISSIONS],
      },
    ],
  };
}

function json(route: Route, data: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

function apiErrorBody(code: string, message: string, details: Record<string, unknown>[] = []) {
  return { error: { code, message, details } };
}

let customerIdCounter = 0;

function makeCustomer(overrides: Partial<CustomerListItem> = {}): CustomerListItem {
  const id = `customer-${++customerIdCounter}`;
  return {
    id,
    display_name: `Customer ${customerIdCounter}`,
    email: `customer${customerIdCounter}@test.com`,
    phone: `+1555${String(customerIdCounter).padStart(7, '0')}`,
    channels: ['email'],
    created_at: new Date(Date.now() - customerIdCounter * 86400000).toISOString(),
    updated_at: new Date().toISOString(),
    ...overrides,
  };
}

function makeCustomerDetail(
  listItem: CustomerListItem,
  overrides: Partial<CustomerDetail> = {},
): CustomerDetail {
  return {
    ...listItem,
    identifiers: [
      { id: `ident-${listItem.id}`, channel: 'email', identifier: listItem.email ?? '' },
    ],
    metadata: {},
    ...overrides,
  };
}

function makeConversation(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  return {
    id: `conv-${Math.random().toString(36).slice(2, 10)}`,
    channel: 'web_chat',
    status: 'open',
    last_activity_at: new Date().toISOString(),
    created_at: new Date(Date.now() - 3600000).toISOString(),
    ...overrides,
  };
}

type CustomersState = {
  currentUser: CurrentUser | null;
  activeTenantId: string | null;
  customers: CustomerListItem[];
  details: Record<string, CustomerDetail>;
  conversations: Record<string, ConversationSummary[]>;
};

function makeState(identity: CurrentUser | null): CustomersState {
  const customers = [
    makeCustomer({
      display_name: 'Sara Ali',
      email: 'sara@example.com',
      phone: '+201001234567',
      channels: ['email', 'whatsapp'],
    }),
    makeCustomer({
      display_name: 'Bob Zhao',
      email: 'bob@example.com',
      phone: '+15550000002',
      channels: ['email'],
    }),
    makeCustomer({
      display_name: 'Clara Diaz',
      email: 'clara@example.com',
      phone: '+15550000003',
      channels: ['email', 'phone'],
    }),
    makeCustomer({
      display_name: 'David Kim',
      email: 'david@example.com',
      phone: '+15550000004',
      channels: ['web_chat'],
    }),
    makeCustomer({
      display_name: 'Elena Rossi',
      email: 'elena@example.com',
      phone: '+15550000005',
      channels: ['email'],
    }),
    makeCustomer({
      display_name: 'Frank Okafor',
      email: 'frank@example.com',
      phone: '+15550000006',
      channels: ['whatsapp'],
    }),
    makeCustomer({
      display_name: 'Grace Chen',
      email: 'grace@example.com',
      phone: '+15550000007',
      channels: ['email'],
    }),
    makeCustomer({
      display_name: 'Henrik Johansson',
      email: 'henrik@example.com',
      phone: '+15550000008',
      channels: ['phone'],
    }),
    makeCustomer({
      display_name: 'Irina Petrov',
      email: 'irina@example.com',
      phone: '+15550000009',
      channels: ['email', 'telegram'],
    }),
    makeCustomer({
      display_name: 'Jorge Santos',
      email: 'jorge@example.com',
      phone: '+15550000010',
      channels: ['email'],
    }),
    makeCustomer({
      display_name: 'Keiko Tanaka',
      email: 'keiko@example.com',
      phone: '+15550000011',
      channels: ['web_chat'],
    }),
    makeCustomer({
      display_name: 'Leila Mansour',
      email: 'leila@example.com',
      phone: '+15550000012',
      channels: ['email', 'whatsapp'],
    }),
    makeCustomer({
      display_name: 'Minh Tran',
      email: 'minh@example.com',
      phone: '+15550000013',
      channels: ['email'],
    }),
    makeCustomer({
      display_name: 'Nina Patel',
      email: 'nina@example.com',
      phone: '+15550000014',
      channels: ['phone'],
    }),
    makeCustomer({
      display_name: 'Omar Hassan',
      email: 'omar@example.com',
      phone: '+15550000015',
      channels: ['email'],
    }),
  ];

  const details: Record<string, CustomerDetail> = {};
  const conversations: Record<string, ConversationSummary[]> = {};

  for (const c of customers) {
    details[c.id] = makeCustomerDetail(c);
    conversations[c.id] = [];
  }

  return {
    currentUser: identity,
    activeTenantId: customerTenant.id,
    customers,
    details,
    conversations,
  };
}

async function installCustomersApi(page: Page, state: CustomersState) {
  await page.route('**/api/v1/tenant/customers/**', async (route) => {
    if (route.request().method() === 'DELETE') {
      return json(route, apiErrorBody('method_not_allowed', 'Method not allowed'), 405);
    }
    return route.fallback();
  });

  await page.route('**/api/v1/tenant/customers', async (route) => {
    if (route.request().method() === 'PUT') {
      return json(route, apiErrorBody('method_not_allowed', 'Method not allowed'), 405);
    }
    return route.fallback();
  });

  await page.context().route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    const method = route.request().method();
    const user = state.currentUser;

    if (path === '/me') {
      if (!user) {
        return json(route, apiErrorBody('unauthenticated', 'Authentication required'), 401);
      }
      return json(route, user);
    }

    if (!path.startsWith('/tenant/customers')) {
      return json(route, { data: null });
    }

    const segments = path.split('/').filter(Boolean);
    const customerId = segments[2];
    const subResource = segments[3];

    if (customerId && subResource === 'conversations' && method === 'GET') {
      const convs = state.conversations[customerId] ?? [];
      return json(route, {
        data: convs,
        pagination: { next_cursor: null, has_more: convs.length > 0 },
      });
    }

    if (customerId && !subResource && method === 'GET') {
      const detail = state.details[customerId];
      if (!detail) {
        return json(route, apiErrorBody('not_found', 'Customer not found'), 404);
      }
      return json(route, { data: detail });
    }

    if (customerId && !subResource && method === 'PATCH') {
      const detail = state.details[customerId];
      if (!detail) {
        return json(route, apiErrorBody('not_found', 'Customer not found'), 404);
      }
      const body = route.request().postDataJSON() as Partial<CustomerDetail>;
      Object.assign(detail, body);
      detail.updated_at = new Date().toISOString();
      return json(route, { data: detail });
    }

    if (customerId && !subResource && method !== 'GET' && method !== 'PATCH') {
      return json(route, apiErrorBody('method_not_allowed', 'Method not allowed'), 405);
    }

    if (!customerId && method === 'POST') {
      const body = route.request().postDataJSON() as {
        display_name: string;
        email?: string;
        phone?: string;
        identifiers?: { channel: CustomerChannel; identifier: string }[];
        metadata?: Record<string, string>;
      };
      const newCustomer = makeCustomer({
        display_name: body.display_name,
        email: body.email ?? null,
        phone: body.phone ?? null,
      });
      const newDetail = makeCustomerDetail(newCustomer, {
        identifiers:
          body.identifiers?.map((ident) => ({
            id: `ident-${newCustomer.id}-${ident.channel}`,
            ...ident,
          })) ?? [],
        metadata: body.metadata ?? {},
      });
      state.customers.unshift(newCustomer);
      state.details[newCustomer.id] = newDetail;
      state.conversations[newCustomer.id] = [];
      return json(route, { data: newDetail }, 201);
    }

    if (!customerId && method === 'GET') {
      let items = [...state.customers];
      const q = url.searchParams.get('q')?.toLowerCase();
      if (q) {
        items = items.filter(
          (c) =>
            c.display_name.toLowerCase().includes(q) ||
            c.email?.toLowerCase().includes(q) ||
            c.phone?.includes(q),
        );
      }
      const cursor = url.searchParams.get('cursor');
      const limit = parseInt(url.searchParams.get('limit') ?? '10', 10);
      let startIndex = 0;
      if (cursor) {
        const cursorIndex = items.findIndex((c) => c.id === cursor);
        if (cursorIndex !== -1) {
          startIndex = cursorIndex + 1;
        }
      }
      const pageItems = items.slice(startIndex, startIndex + limit);
      const hasMore = startIndex + limit < items.length;
      const nextCursor =
        hasMore && pageItems.length > 0 ? pageItems[pageItems.length - 1].id : null;
      return json(route, {
        data: pageItems,
        pagination: { next_cursor: nextCursor, has_more: hasMore },
      });
    }

    if (customerId && subResource && subResource !== 'conversations') {
      return json(route, apiErrorBody('not_found', 'Not found'), 404);
    }

    if (!customerId && method !== 'GET' && method !== 'POST') {
      return json(route, apiErrorBody('method_not_allowed', 'Method Not Allowed'), 405);
    }

    return json(route, { data: null });
  });
}

test.describe('Customer Profiles', () => {
  test('Customer list pagination and search', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.goto('/tenant/customers');

    await expect(page.getByRole('heading', { name: 'Customers' })).toBeVisible();

    const firstPage = state.customers.slice(0, 10);
    const secondPage = state.customers.slice(10, 15);

    for (const c of firstPage) {
      await expect(page.getByText(c.display_name)).toBeVisible();
    }
    for (const c of secondPage) {
      await expect(page.getByText(c.display_name)).toHaveCount(0);
    }

    const loadMore = page.getByRole('button', { name: 'Load more' });
    if (await loadMore.isVisible()) {
      const loadMoreResponse = page.waitForResponse(
        (res) => res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'GET',
      );
      await loadMore.click();
      await loadMoreResponse;
    }

    for (const c of state.customers) {
      await expect(page.getByText(c.display_name)).toBeVisible();
    }

    const searchInput = page.getByPlaceholder('Search customers');
    if (await searchInput.isVisible()) {
      const searchResponse = page.waitForResponse(
        (res) => res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'GET',
      );
      await searchInput.fill('Sara');
      await searchResponse;
      await expect(page.getByText('Sara Ali')).toBeVisible();
      for (const c of state.customers.slice(1)) {
        await expect(page.getByText(c.display_name)).toHaveCount(0);
      }
      const clearResponse = page.waitForResponse(
        (res) => res.url().includes('/api/v1/tenant/customers') && res.request().method() === 'GET',
      );
      await searchInput.clear();
      await clearResponse;
    }

    for (const c of firstPage) {
      await expect(page.getByText(c.display_name)).toBeVisible();
    }
  });

  test('Profile display', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    const target = state.customers[0];
    state.details[target.id] = makeCustomerDetail(target, {
      identifiers: [
        { id: 'ident-1', channel: 'email', identifier: 'sara@example.com' },
        { id: 'ident-2', channel: 'whatsapp', identifier: '+201001234567' },
      ],
      metadata: { plan: 'enterprise', region: 'EMEA' },
    });
    state.conversations[target.id] = [
      makeConversation({ id: 'conv-1', channel: 'web_chat', status: 'open' }),
      makeConversation({ id: 'conv-2', channel: 'email', status: 'closed' }),
    ];

    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.goto(`/tenant/customers/${target.id}`);

    await expect(page.getByText(target.display_name)).toBeVisible();
    await expect(page.getByText(target.email!)).toBeVisible();
    await expect(page.getByText(target.phone!)).toBeVisible();

    await expect(page.getByText('sara@example.com')).toBeVisible();
    await expect(page.getByText('+201001234567')).toBeVisible();

    await expect(page.getByText('enterprise')).toBeVisible();
    await expect(page.getByText('EMEA')).toBeVisible();

    await expect(page.getByText('Open')).toBeVisible();
    await expect(page.getByText('Closed')).toBeVisible();
  });

  test('Profile display with empty conversations', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    const target = state.customers[0];
    state.conversations[target.id] = [];

    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.goto(`/tenant/customers/${target.id}`);

    await expect(page.getByText('No conversations yet')).toBeVisible();
  });

  test('Create customer flow', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.goto('/tenant/customers');

    const newButton = page.getByRole('button', { name: 'New customer' });
    await expect(newButton).toBeVisible();

    const responsePromise = page.waitForResponse(
      (res) =>
        res.url().includes('/api/v1/tenant/customers') &&
        res.request().method() === 'POST' &&
        res.status() === 201,
    );

    await newButton.click();
    await page.getByLabel('Display name').fill('New Person');
    await page.getByLabel('Email').fill('new@test.com');
    await page.getByLabel('Phone').fill('+15550000999');

    await page.getByRole('button', { name: 'Add identifier' }).click();
    await page.getByLabel('Channel').fill('whatsapp');
    await page.getByLabel('Identifier value').fill('+15550000999');

    await page.getByRole('button', { name: 'Add metadata' }).click();
    await page.getByLabel('Key').fill('source');
    await page.getByLabel('Value').fill('web');

    await page.getByRole('button', { name: 'Create' }).click();

    const response = await responsePromise;
    const body = await response.json();
    expect(body.data.display_name).toBe('New Person');
    expect(body.data.email).toBe('new@test.com');

    await expect(page.getByText('New Person')).toBeVisible();
  });

  test('Update customer flow', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    const target = state.details[state.customers[0].id];
    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.goto(`/tenant/customers/${target.id}`);

    const editButton = page.getByRole('button', { name: 'Edit' });
    await expect(editButton).toBeVisible();
    await editButton.click();

    const displayNameInput = page.getByLabel('Display name');
    await displayNameInput.clear();
    await displayNameInput.fill('Sara Updated');

    const responsePromise = page.waitForResponse(
      (res) =>
        res.url().includes(`/api/v1/tenant/customers/${target.id}`) &&
        res.request().method() === 'PATCH' &&
        res.status() === 200,
    );

    await page.getByRole('button', { name: 'Save' }).click();
    const response = await responsePromise;
    expect((await response.json()).data.display_name).toBe('Sara Updated');
  });

  test('Viewer restrictions', async ({ page }) => {
    const state = makeState(makeViewerIdentity());
    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.goto('/tenant/customers');

    await expect(page.getByRole('button', { name: 'New customer' })).toHaveCount(0);

    const target = state.customers[0];
    await page.goto(`/tenant/customers/${target.id}`);

    await expect(page.getByRole('button', { name: 'Edit' })).toHaveCount(0);
  });

  test('Cross-tenant not-found', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.route(`**/api/v1/tenant/customers/nonexistent-id`, async (route) => {
      await route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify(apiErrorBody('not_found', 'Customer not found')),
      });
    });

    await page.goto('/tenant/customers/nonexistent-id');
    await expect(page.getByText('Customer not found')).toBeVisible();
  });

  test('Conflict display', async ({ page }) => {
    const state = makeState(makeManagerIdentity());
    await installCustomersApi(page, state);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, customerTenant);

    await page.route(`**/api/v1/tenant/customers`, async (route) => {
      if (route.request().method() === 'POST') {
        const body = route.request().postDataJSON() as { display_name: string };
        if (body.display_name === 'Conflict Person') {
          return route.fulfill({
            status: 409,
            contentType: 'application/json',
            body: JSON.stringify(
              apiErrorBody('conflict', 'Channel identifier already held by another customer.', [
                {
                  field: 'identifiers',
                  channel: 'whatsapp',
                  identifier: '+15550000999',
                  existing_customer_id: 'existing-customer-id',
                  existing_customer_name: 'Existing Customer',
                },
              ]),
            ),
          });
        }
        return route.fallback();
      }
      return route.fallback();
    });

    await page.goto('/tenant/customers');

    await page.getByRole('button', { name: 'New customer' }).click();
    await page.getByLabel('Display name').fill('Conflict Person');
    await page.getByLabel('Email').fill('conflict@test.com');
    await page.getByRole('button', { name: 'Create' }).click();

    await expect(page.getByText('already held by another customer')).toBeVisible();
  });
});
