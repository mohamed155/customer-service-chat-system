import { expect, Page, Route, test } from '@playwright/test';

type ConversationStatus = 'open' | 'pending' | 'resolved' | 'closed';
type ConversationChannel = 'email' | 'phone' | 'web_chat' | 'whatsapp' | 'telegram';
type MemberRole = 'agent' | 'viewer';

interface ConversationWire {
  readonly id: string;
  readonly customer: { readonly id: string; readonly display_name: string };
  readonly channel: string;
  readonly status: ConversationStatus;
  readonly assignee: {
    readonly membership_id: string;
    readonly display_name: string;
    readonly active: boolean;
  } | null;
  readonly last_message: { readonly kind: string; readonly preview: string } | null;
  readonly last_activity_at: string;
  readonly created_at: string;
}

interface MessageWire {
  readonly id: string;
  readonly kind: string;
  readonly sender: { readonly type: string; readonly display_name: string };
  readonly body: string;
  readonly created_at: string;
}

interface PaginatedResponse<T> {
  readonly data: T[];
  readonly pagination: { readonly next_cursor: string | null; readonly has_more: boolean };
}

function agentIdentity() {
  return {
    id: 'user-agent-1',
    email: 'agent@test.com',
    displayName: 'Alice Agent',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: 'tenant-conv',
        tenantName: 'Conv Co',
        tenantSlug: 'conv-co',
        role: 'agent' as MemberRole,
        permissions: ['conversations.view', 'conversations.manage'],
      },
    ],
  };
}

function viewerIdentity() {
  return {
    id: 'user-viewer-1',
    email: 'viewer@test.com',
    displayName: 'Victor Viewer',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: 'tenant-conv',
        tenantName: 'Conv Co',
        tenantSlug: 'conv-co',
        role: 'viewer' as MemberRole,
        permissions: ['conversations.view'],
      },
    ],
  };
}

const testTenant = {
  id: 'tenant-conv',
  name: 'Conv Co',
  slug: 'conv-co',
  status: 'active' as const,
  plan: 'starter' as const,
};

function json(route: Route, data: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

function apiError(code: string, message: string) {
  return { error: { code, message, details: [] } };
}

let convCounter = 0;

function makeConversation(overrides: Partial<ConversationWire> = {}): ConversationWire {
  const id = `conv-e2e-${++convCounter}`;
  return {
    id,
    customer: { id: `cust-${id}`, display_name: `Customer ${convCounter}` },
    channel: 'web_chat',
    status: 'open',
    assignee: null,
    last_message: null,
    last_activity_at: new Date().toISOString(),
    created_at: new Date(Date.now() - 3600000).toISOString(),
    ...overrides,
  };
}

function makeMessage(overrides: Partial<MessageWire> = {}): MessageWire {
  return {
    id: `msg-${Math.random().toString(36).slice(2, 10)}`,
    kind: 'reply',
    sender: { type: 'customer', display_name: 'Test Customer' },
    body: 'Test message body.',
    created_at: new Date().toISOString(),
    ...overrides,
  };
}

async function installApi(page: Page, identity: ReturnType<typeof agentIdentity>) {
  const inbox: ConversationWire[] = [
    makeConversation({ id: 'conv-e2e-open', status: 'open', channel: 'web_chat' }),
    makeConversation({ id: 'conv-e2e-pending', status: 'pending', channel: 'email' }),
    makeConversation({ id: 'conv-e2e-resolved', status: 'resolved', channel: 'whatsapp' }),
    makeConversation({ id: 'conv-e2e-closed', status: 'closed', channel: 'telegram' }),
  ];
  const messages: Record<string, MessageWire[]> = {
    'conv-e2e-open': [makeMessage({ body: 'I need help with my order.' })],
  };

  await page.context().route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    const method = route.request().method();

    if (path === '/me') {
      return json(route, identity);
    }

    if (path === '/tenant/conversations' && method === 'GET') {
      const statusFilter = url.searchParams.get('status') || 'open';
      let filtered = inbox;
      if (statusFilter !== 'all') {
        filtered = inbox.filter((c) => c.status === statusFilter);
      }
      return json(route, {
        data: filtered,
        pagination: { next_cursor: null, has_more: false },
      } satisfies PaginatedResponse<ConversationWire>);
    }

    if (path === '/tenant/conversations' && method === 'POST') {
      const body = route.request().postDataJSON() as {
        customer_id: string;
        channel: string;
        message: { body: string };
      };
      const newConv = makeConversation({
        customer: { id: body.customer_id, display_name: 'New Customer' },
        channel: body.channel,
        status: 'open',
        last_message: { kind: 'reply', preview: body.message.body.slice(0, 80) },
      });
      return json(
        route,
        {
          data: {
            ...newConv,
            messages: [makeMessage({ body: body.message.body, kind: 'reply' })],
          },
        },
        201,
      );
    }

    const convMatch = path.match(/^\/tenant\/conversations\/([^/]+)$/);
    const msgMatch = path.match(/^\/tenant\/conversations\/([^/]+)\/messages$/);

    if (convMatch && method === 'GET') {
      const conv = inbox.find((c) => c.id === convMatch[1]);
      if (!conv) return json(route, apiError('not_found', 'Conversation not found'), 404);
      return json(route, {
        data: {
          ...conv,
          participants: [
            { type: 'customer', id: conv.customer.id, display_name: conv.customer.display_name },
          ],
        },
      });
    }

    if (convMatch && method === 'PATCH') {
      const conv = inbox.find((c) => c.id === convMatch[1]);
      if (!conv) return json(route, apiError('not_found', 'Conversation not found'), 404);
      const patch = route.request().postDataJSON() as { status?: ConversationStatus };
      if (patch.status) conv.status = patch.status;
      return json(route, { data: conv });
    }

    if (msgMatch && method === 'GET') {
      const convMsgs = messages[msgMatch[1]] || [];
      return json(route, {
        data: convMsgs,
        pagination: { next_cursor: null, has_more: false },
      } satisfies PaginatedResponse<MessageWire>);
    }

    if (msgMatch && method === 'POST') {
      const body = route.request().postDataJSON() as { kind: string; body: string };
      const msg = makeMessage({ kind: body.kind, body: body.body });
      if (!messages[msgMatch[1]]) messages[msgMatch[1]] = [];
      messages[msgMatch[1]].push(msg);
      return json(route, {
        data: {
          message: msg,
          conversation: { status: 'open', last_activity_at: new Date().toISOString() },
        },
      });
    }

    return json(route, { data: null });
  });
}

test.describe('Conversation Core', () => {
  test('Agent: inbox lists conversations with all statuses', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    await expect(page.getByRole('heading', { name: 'Conversations' })).toBeVisible();

    for (const status of ['open', 'pending', 'resolved', 'closed'] as ConversationStatus[]) {
      await expect(page.getByText(status, { exact: false }).first()).toBeVisible();
    }
  });

  test('Agent: can view conversation detail', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-open');

    await expect(page.getByText('I need help with my order.')).toBeVisible();
  });

  test('Agent: status patch updates displayed status', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-open');

    const patchResponse = page.waitForResponse(
      (res) =>
        res.url().includes('/api/v1/tenant/conversations/conv-e2e-open') &&
        res.request().method() === 'PATCH',
    );

    await page.getByRole('button', { name: 'Resolve' }).click();
    await patchResponse;

    await expect(page.getByText('resolved', { exact: false }).first()).toBeVisible();
  });

  test('Agent: can add a reply', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-open');

    const postResponse = page.waitForResponse(
      (res) =>
        res.url().includes('/api/v1/tenant/conversations/conv-e2e-open/messages') &&
        res.request().method() === 'POST',
    );

    await page.getByPlaceholder('Type a message').fill('Thanks for your patience.');
    await page.getByRole('button', { name: 'Send' }).click();
    await postResponse;

    await expect(page.getByText('Thanks for your patience.')).toBeVisible();
  });

  test('Viewer: can view inbox and detail but not interact', async ({ page }) => {
    await installApi(page, viewerIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    await expect(page.getByRole('heading', { name: 'Conversations' })).toBeVisible();

    await page.goto('/tenant/conversations/conv-e2e-open');
    await expect(page.getByText('I need help with my order.')).toBeVisible();

    await expect(page.getByRole('button', { name: 'Send' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Resolve' })).toHaveCount(0);
  });
});
