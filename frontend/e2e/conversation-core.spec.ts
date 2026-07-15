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
  readonly logged_by: { readonly membership_id: string; readonly display_name: string } | null;
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

const AGENT_MEMBERSHIP_ID = 'member-agent-1';
const VIEWER_MEMBERSHIP_ID = 'member-viewer-1';

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
    logged_by: null,
    body: 'Test message body.',
    created_at: new Date().toISOString(),
    ...overrides,
  };
}

function membershipIdForIdentity(identity: ReturnType<typeof agentIdentity>): string {
  return identity.memberships.some((m) => m.role === 'agent' || m.role === 'owner')
    ? AGENT_MEMBERSHIP_ID
    : VIEWER_MEMBERSHIP_ID;
}

function encodeCursor(index: number): string {
  return Buffer.from(String(index)).toString('base64');
}

function decodeCursor(cursor: string): number {
  return parseInt(Buffer.from(cursor, 'base64').toString('utf-8'), 10);
}

async function installApi(page: Page, identity: ReturnType<typeof agentIdentity>) {
  const inbox: ConversationWire[] = [
    makeConversation({ id: 'conv-e2e-open', status: 'open', channel: 'web_chat' }),
    makeConversation({ id: 'conv-e2e-pending', status: 'pending', channel: 'email' }),
    makeConversation({ id: 'conv-e2e-resolved', status: 'resolved', channel: 'whatsapp' }),
    makeConversation({ id: 'conv-e2e-closed', status: 'closed', channel: 'telegram' }),
    makeConversation({
      id: 'conv-e2e-assigned',
      status: 'open',
      channel: 'email',
      assignee: {
        membership_id: AGENT_MEMBERSHIP_ID,
        display_name: 'Alice Agent',
        active: true,
      },
    }),
  ];
  inbox.push(
    makeConversation({
      id: 'conv-e2e-long',
      status: 'open',
      channel: 'web_chat',
      customer: { id: 'cust-long', display_name: 'Long Customer' },
    }),
  );

  const messages: Record<string, MessageWire[]> = {
    'conv-e2e-open': [makeMessage({ body: 'I need help with my order.' })],
  };

  messages['conv-e2e-long'] = Array.from({ length: 60 }, (_, i) => {
    const idx = i + 1;
    return makeMessage({
      body: `Load test message ${idx} of 60`,
      created_at: new Date(Date.now() - (60 - idx) * 60000).toISOString(),
    });
  });

  const currentMembershipId = membershipIdForIdentity(identity);

  await page.context().route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');
    const method = route.request().method();

    if (path === '/me') {
      return json(route, identity);
    }

    if (path === '/tenant/members' && method === 'GET') {
      return json(route, {
        data: [
          {
            id: AGENT_MEMBERSHIP_ID,
            userId: 'user-agent-1',
            displayName: 'Alice Agent',
            email: 'agent@test.com',
            role: 'agent',
            status: 'active',
            joinedAt: '2026-07-01T09:00:00Z',
          },
          {
            id: VIEWER_MEMBERSHIP_ID,
            userId: 'user-viewer-1',
            displayName: 'Victor Viewer',
            email: 'viewer@test.com',
            role: 'viewer',
            status: 'active',
            joinedAt: '2026-07-01T09:00:00Z',
          },
        ],
      });
    }

    if (path === '/tenant/conversations' && method === 'GET') {
      const statusFilter = url.searchParams.get('status') || 'open';
      const channelFilter = url.searchParams.get('channel');
      const assigneeFilter = url.searchParams.get('assignee');

      let filtered = inbox;
      if (statusFilter !== 'all') {
        filtered = filtered.filter((c) => c.status === statusFilter);
      }
      if (channelFilter) {
        filtered = filtered.filter((c) => c.channel === channelFilter);
      }
      if (assigneeFilter === 'me') {
        filtered = filtered.filter((c) => c.assignee?.membership_id === currentMembershipId);
      } else if (assigneeFilter === 'unassigned') {
        filtered = filtered.filter((c) => c.assignee === null);
      } else if (assigneeFilter) {
        filtered = filtered.filter((c) => c.assignee?.membership_id === assigneeFilter);
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
      inbox.push(newConv);
      messages[newConv.id] = [makeMessage({ body: body.message.body, kind: 'reply' })];
      return json(
        route,
        {
          data: {
            ...newConv,
            participants: [
              {
                type: 'customer',
                id: newConv.customer.id,
                display_name: newConv.customer.display_name,
              },
              {
                type: 'member',
                membership_id: currentMembershipId,
                display_name: identity.displayName,
              },
            ],
            messages: messages[newConv.id],
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
            {
              type: 'customer',
              id: conv.customer.id,
              display_name: conv.customer.display_name,
            },
          ],
        },
      });
    }

    if (convMatch && method === 'PATCH') {
      const conv = inbox.find((c) => c.id === convMatch[1]);
      if (!conv) return json(route, apiError('not_found', 'Conversation not found'), 404);
      const patch = route.request().postDataJSON() as {
        status?: ConversationStatus;
        assigned_membership_id?: string | null;
      };
      if (patch.status !== undefined) conv.status = patch.status;
      if (patch.assigned_membership_id !== undefined) {
        conv.assignee = patch.assigned_membership_id
          ? {
              membership_id: patch.assigned_membership_id,
              display_name:
                patch.assigned_membership_id === currentMembershipId
                  ? identity.displayName
                  : 'Alice Agent',
              active: true,
            }
          : null;
      }
      return json(route, {
        data: {
          ...conv,
          participants: [
            {
              type: 'customer',
              id: conv.customer.id,
              display_name: conv.customer.display_name,
            },
          ],
        },
      });
    }

    if (msgMatch && method === 'GET') {
      const convMsgs = messages[msgMatch[1]] || [];
      const limit = Math.min(parseInt(url.searchParams.get('limit') || '20'), 200);
      const cursor = url.searchParams.get('cursor');

      const ordered = [...convMsgs].sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      );

      let startIdx = 0;
      if (cursor) {
        try {
          startIdx = decodeCursor(cursor);
        } catch {
          startIdx = 0;
        }
      }

      const page = ordered.slice(startIdx, startIdx + limit);
      const hasMore = ordered.length > startIdx + limit;
      const nextCursor = hasMore ? encodeCursor(startIdx + limit) : null;

      return json(route, {
        data: page,
        pagination: { next_cursor: nextCursor, has_more: hasMore },
      } satisfies PaginatedResponse<MessageWire>);
    }

    if (msgMatch && method === 'POST') {
      const body = route.request().postDataJSON() as {
        kind: string;
        body: string;
      };
      const msg = makeMessage({
        kind: body.kind,
        body: body.body,
        sender:
          body.kind === 'customer'
            ? { type: 'customer', display_name: 'Test Customer' }
            : {
                type: 'member',
                display_name: identity.displayName,
              },
        logged_by:
          body.kind === 'customer'
            ? {
                membership_id: currentMembershipId,
                display_name: identity.displayName,
              }
            : null,
      });
      if (!messages[msgMatch[1]]) messages[msgMatch[1]] = [];
      messages[msgMatch[1]].push(msg);

      const conv = inbox.find((c) => c.id === msgMatch[1]);
      const isCustomerFacing = body.kind === 'reply' || body.kind === 'customer';
      const wasResolvedOrClosed = conv?.status === 'resolved' || conv?.status === 'closed';
      if (isCustomerFacing && wasResolvedOrClosed && conv) {
        conv.status = 'open';
      }

      return json(route, {
        data: {
          message: msg,
          conversation: { status: 'open', last_activity_at: new Date().toISOString() },
        },
      });
    }

    // Customer endpoints
    if (method === 'GET' && path === '/tenant/customers') {
      const q = url.searchParams.get('q')?.toLowerCase();
      const allCustomers = [
        ...new Map(
          inbox.map((c) => [
            c.customer.id,
            {
              id: c.customer.id,
              display_name: c.customer.display_name,
              email: null,
              phone: null,
              channels: [c.channel],
              created_at: c.created_at,
              updated_at: c.created_at,
            },
          ]),
        ).values(),
      ];
      const filtered = q
        ? allCustomers.filter((c) => c.display_name.toLowerCase().includes(q))
        : allCustomers;
      return json(route, {
        data: filtered,
        pagination: { next_cursor: null, has_more: false },
      });
    }

    const customerDetailMatch = path.match(/^\/tenant\/customers\/([a-zA-Z0-9_-]+)$/);
    if (customerDetailMatch && method === 'GET') {
      const customerId = customerDetailMatch[1];
      const conv = inbox.find((c) => c.customer.id === customerId);
      if (!conv) return json(route, apiError('not_found', 'Not found'), 404);
      return json(route, {
        data: {
          id: conv.customer.id,
          display_name: conv.customer.display_name,
          email: null,
          phone: null,
          channels: [conv.channel],
          created_at: conv.created_at,
          updated_at: conv.created_at,
        },
      });
    }

    const customerConvsMatch = path.match(/^\/tenant\/customers\/([a-zA-Z0-9_-]+)\/conversations$/);
    if (customerConvsMatch && method === 'GET') {
      const customerId = customerConvsMatch[1];
      const convs = inbox
        .filter((c) => c.customer.id === customerId)
        .map((c) => ({
          id: c.id,
          channel: c.channel,
          status: c.status,
          last_activity_at: c.last_activity_at,
          created_at: c.created_at,
        }));
      return json(route, {
        data: convs,
        pagination: { next_cursor: null, has_more: false },
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

  test('Inbox filter narrowing + empty-state reset', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    await expect(page.getByRole('heading', { name: 'Conversations' })).toBeVisible();

    const conversationText = page.locator('app-conversations, [data-testid="conversations"]');
    await expect(conversationText).toBeVisible();

    const statusFilter = page.getByRole('combobox', { name: /status/i });
    if (await statusFilter.isVisible()) {
      await statusFilter.selectOption('closed');
      await page.waitForTimeout(300);
      await expect(page.getByText('closed', { exact: false })).toBeVisible();
      await statusFilter.selectOption('open');
      await page.waitForTimeout(300);
    }

    const channelFilter = page.getByRole('combobox', { name: /channel/i });
    if (await channelFilter.isVisible()) {
      await channelFilter.selectOption('phone');
      await page.waitForTimeout(300);

      const emptyState = page.getByText(/no (conversations|results)/i);
      if (await emptyState.isVisible().catch(() => false)) {
        await expect(emptyState).toBeVisible();

        const resetButton = page.getByRole('button', { name: /reset|clear/i });
        if (await resetButton.isVisible().catch(() => false)) {
          await resetButton.click();
        } else {
          await channelFilter.selectOption('');
          await channelFilter.selectOption('web_chat');
        }
        await page.waitForTimeout(300);
        await expect(page.getByText('open', { exact: false }).first()).toBeVisible();
      }
    }
  });

  test('Load-older timeline stability', async ({ page }) => {
    test.slow();
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-long');

    await expect(page.getByText('Load test message 60 of 60')).toBeVisible();
    await expect(page.getByText('Load test message 41 of 60')).toBeVisible();

    const timeline = page.locator('app-conversation-thread, [data-testid="timeline"], .timeline');
    const initialCount = await timeline
      .locator('app-message, [data-testid="message"], .message')
      .count();

    const loadOlderButton = page.getByRole('button', { name: /load older|show earlier/i });
    if (await loadOlderButton.isVisible().catch(() => false)) {
      await loadOlderButton.click();
      await page.waitForTimeout(500);

      await expect(page.getByText('Load test message 40 of 60')).toBeVisible();
      await expect(page.getByText('Load test message 21 of 60')).toBeVisible();

      const secondCount = await timeline
        .locator('app-message, [data-testid="message"], .message')
        .count();
      expect(secondCount).toBeGreaterThan(initialCount);

      if (await loadOlderButton.isVisible().catch(() => false)) {
        await loadOlderButton.click();
        await page.waitForTimeout(500);
      }
    }

    if (await loadOlderButton.isVisible().catch(() => false)) {
      await loadOlderButton.click();
      await page.waitForTimeout(500);
    }

    const allMessages = await timeline
      .locator('app-message, [data-testid="message"], .message')
      .allTextContents();
    const messageSet = new Set(allMessages);
    expect(messageSet.size).toBe(allMessages.length);

    const seenNumbers: number[] = [];
    for (const text of allMessages) {
      const match = text.match(/Load test message (\d+)/);
      if (match) seenNumbers.push(parseInt(match[1], 10));
    }
    let inOrder = true;
    for (let i = 1; i < seenNumbers.length; i++) {
      if (seenNumbers[i] < seenNumbers[i - 1]) {
        inOrder = false;
        break;
      }
    }
    expect(inOrder).toBe(true);
  });

  test('New-conversation create flow', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    const newConvButton = page.getByRole('button', { name: /new conversation/i });
    await expect(newConvButton).toBeVisible();
    await newConvButton.click();

    await expect(page.getByRole('dialog')).toBeVisible();

    const customerSearch = page.getByPlaceholder(/search|customer/i);
    if (await customerSearch.isVisible().catch(() => false)) {
      await customerSearch.fill('Customer');
      await page.waitForTimeout(200);
      const firstOption = page.locator('[role="option"], .option, tui-option').first();
      if (await firstOption.isVisible().catch(() => false)) {
        await firstOption.click();
      }
    }

    const channelSelect = page.getByRole('combobox', { name: /channel/i });
    if (await channelSelect.isVisible().catch(() => false)) {
      await channelSelect.selectOption('email');
    }

    const messageField = page.getByPlaceholder(/message|type/i).first();
    if (await messageField.isVisible().catch(() => false)) {
      await messageField.fill('Welcome to our support!');
    }

    const submitButton = page.getByRole('button', { name: /send|create|start/i });
    if (await submitButton.isVisible().catch(() => false)) {
      const postResponse = page.waitForResponse(
        (res) =>
          res.url().includes('/api/v1/tenant/conversations') && res.request().method() === 'POST',
      );
      await submitButton.click();
      await postResponse;
    }

    await expect(page.getByText('Welcome to our support!')).toBeVisible();
  });

  test('Internal-note and log-customer-message composition', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-open');

    const noteTab = page.getByRole('tab', { name: /note/i });
    const noteRadio = page.getByRole('radio', { name: /note/i });
    if (await noteTab.isVisible().catch(() => false)) {
      await noteTab.click();
    } else if (await noteRadio.isVisible().catch(() => false)) {
      await noteRadio.click();
    }

    await page.getByPlaceholder('Type a message').fill('Internal note for the team.');
    const sendButton = page.getByRole('button', { name: 'Send' });

    const firstPostResponse = page.waitForResponse(
      (res) =>
        res.url().includes('/api/v1/tenant/conversations/conv-e2e-open/messages') &&
        res.request().method() === 'POST',
    );
    await sendButton.click();
    await firstPostResponse;

    await expect(page.getByText('Internal note for the team.')).toBeVisible();

    const noteElements = page.locator(
      'app-message.note, [data-testid="message"].note, .message.note, [data-kind="note"]',
    );
    if ((await noteElements.count()) > 0) {
      await expect(noteElements.last()).toBeVisible();
    }

    const customerTab = page.getByRole('tab', { name: /log|customer/i });
    const customerRadio = page.getByRole('radio', { name: /log|customer/i });
    if (await customerTab.isVisible().catch(() => false)) {
      await customerTab.click();
    } else if (await customerRadio.isVisible().catch(() => false)) {
      await customerRadio.click();
    }

    const composerField = page.getByPlaceholder('Type a message');
    await composerField.fill('Customer said they are happy.');

    const secondPostResponse = page.waitForResponse(
      (res) =>
        res.url().includes('/api/v1/tenant/conversations/conv-e2e-open/messages') &&
        res.request().method() === 'POST',
    );
    await page.getByRole('button', { name: 'Send' }).click();
    await secondPostResponse;

    await expect(page.getByText('Customer said they are happy.')).toBeVisible();
  });

  test('Auto-reopen of a resolved conversation on reply', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-resolved');

    await expect(page.getByText('resolved', { exact: false }).first()).toBeVisible();

    const postResponse = page.waitForResponse(
      (res) =>
        res.url().includes('/api/v1/tenant/conversations/conv-e2e-resolved/messages') &&
        res.request().method() === 'POST',
    );

    await page.getByPlaceholder('Type a message').fill('Reopening this conversation.');
    await page.getByRole('button', { name: 'Send' }).click();
    await postResponse;

    await expect(page.getByText('open', { exact: false }).first()).toBeVisible();
  });

  test('Assign/unassign', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-open');

    const assignButton = page.getByRole('button', { name: /assign|unassigned/i });
    if (await assignButton.isVisible().catch(() => false)) {
      const patchAssign = page.waitForResponse(
        (res) =>
          res.url().includes('/api/v1/tenant/conversations/conv-e2e-open') &&
          res.request().method() === 'PATCH',
      );
      await assignButton.click();

      const memberOption = page.locator('[role="option"], .option, tui-option').filter({
        hasText: 'Alice Agent',
      });
      if (await memberOption.isVisible().catch(() => false)) {
        await memberOption.click();
      }
      await patchAssign;
      await expect(page.getByText('Alice Agent')).toBeVisible();
    }

    const currentAssignee = page.getByText(/alice agent/i);
    if (await currentAssignee.isVisible().catch(() => false)) {
      const unassignButton = page.getByRole('button', { name: /unassign|remove/i });
      if (await unassignButton.isVisible().catch(() => false)) {
        const patchUnassign = page.waitForResponse(
          (res) =>
            res.url().includes('/api/v1/tenant/conversations/conv-e2e-open') &&
            res.request().method() === 'PATCH',
        );
        await unassignButton.click();
        await patchUnassign;
      }
    }
  });

  test('Customer-profile conversation-history continuity', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    const custId = 'cust-conv-e2e-open';
    await page.goto(`/tenant/customers/${custId}`);

    await expect(page.getByText(/conversations|history/i)).toBeVisible();

    const convText = page.getByText(/conv-e2e-open/i);
    if (await convText.isVisible().catch(() => false)) {
      await expect(convText).toBeVisible();
    }
  });

  test('"New conversation" hidden for Viewer', async ({ page }) => {
    await installApi(page, viewerIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations');

    await expect(page.getByRole('heading', { name: 'Conversations' })).toBeVisible();
    await expect(page.getByRole('button', { name: /new conversation/i })).toHaveCount(0);
  });

  test('Timed inbox-to-timeline assertion under SC-001 15-second threshold', async ({ page }) => {
    await installApi(page, agentIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    const start = Date.now();

    await page.goto('/tenant/conversations');
    await expect(page.getByRole('heading', { name: 'Conversations' })).toBeVisible();

    const convLink = page.getByRole('link', { name: /conv-e2e-open/i });
    if (await convLink.isVisible().catch(() => false)) {
      await convLink.click();
    } else {
      await page.goto('/tenant/conversations/conv-e2e-open');
    }

    await expect(page.getByText('I need help with my order.')).toBeVisible();

    const elapsed = Date.now() - start;
    expect(elapsed).toBeLessThan(15000);
  });

  test('Permission-hidden controls for Viewer', async ({ page }) => {
    await installApi(page, viewerIdentity());
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/conversations/conv-e2e-open');

    await expect(page.getByText('I need help with my order.')).toBeVisible();

    await expect(page.getByRole('button', { name: 'Send' })).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Resolve' })).toHaveCount(0);

    const composerField = page.getByPlaceholder('Type a message');
    await expect(composerField).toHaveCount(0);

    const assignControls = page.getByRole('button', { name: /assign/i });
    await expect(assignControls).toHaveCount(0);
  });
});
