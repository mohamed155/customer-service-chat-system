import AxeBuilder from '@axe-core/playwright';
import { Page, Route } from '@playwright/test';
import { expect, test } from './support/test';
import { HOST_URL, installWidgetHarness } from './support/widget-harness';

/**
 * Automated accessibility checks (MOH-74, WCAG 2.2 AA, constitution principle XII).
 *
 * axe-core runs on the shell and the key pages, in the light and in the dark theme, and the test
 * fails on a violation its rules call serious or critical. A violation that is known and has an
 * issue goes in KNOWN with the issue, and the list may only get shorter: an entry that no longer
 * matches anything fails the test, so it is removed when the problem is fixed.
 */

const TENANT_PERMISSIONS = [
  'overview.view',
  'conversations.view',
  'conversations.manage',
  'customers.view',
  'analytics.view',
  'knowledge_base.view',
  'knowledge_base.manage',
  'members.view',
  'members.manage',
  'settings.view',
];

const tenant = { id: 'tenant-1', name: 'Acme Support', slug: 'acme-support', status: 'active' };

const identities = {
  tenant: {
    id: 'user-1',
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
    staffTenantPermissions: null,
    memberships: [],
  },
};

const NOW = '2026-11-16T09:00:00Z';

const conversation = {
  id: 'conv-1',
  customer: { id: 'cust-1', display_name: 'Maya Chen' },
  channel: 'web_chat',
  status: 'open',
  assignee: null,
  last_message: { kind: 'customer', preview: 'Where is my order?' },
  last_activity_at: NOW,
  created_at: '2026-11-16T08:00:00Z',
};

const message = {
  id: 'msg-1',
  kind: 'customer',
  sender: { type: 'customer', display_name: 'Maya Chen' },
  logged_by: null,
  body: 'Where is my order?',
  created_at: NOW,
};

const member = {
  id: 'member-1',
  userId: 'user-2',
  displayName: 'Alice Agent',
  email: 'alice@acme.test',
  role: 'agent',
  status: 'active',
  joinedAt: '2026-10-01T00:00:00Z',
};

const knowledgeItem = {
  id: 'kb-1',
  itemType: 'article',
  title: 'Returns and exchanges',
  status: 'published',
  categoryId: null,
  categoryName: null,
  createdByDisplay: 'Olivia Owner',
  createdAt: NOW,
  updatedAt: NOW,
  tags: ['returns'],
};

function json(route: Route, body: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
}

const page1 = (items: unknown[]) => ({ items, nextCursor: null, hasMore: false });

interface MockOptions {
  /** API paths whose request is never answered, so the page stays in its loading state. */
  readonly hold?: readonly string[];
  /** The inbox has no conversation. */
  readonly emptyInbox?: boolean;
}

async function mockApi(
  page: Page,
  identity: (typeof identities)[keyof typeof identities],
  options: MockOptions = {},
) {
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    if (path === '/me') return json(route, identity);
    if (options.hold?.includes(path)) return new Promise<void>(() => undefined);
    if (options.emptyInbox && path === '/tenant/conversations') {
      return json(route, { data: [], pagination: { next_cursor: null, has_more: false } });
    }
    if (path === '/platform/tenants') return json(route, page1([tenant]));
    if (path === '/tenant/analytics/summary') {
      return json(route, {
        data: {
          range: { from: '2026-10-18', to: '2026-11-16' },
          channel: null,
          conversation_volume: 12,
          concluded_count: 9,
          ai_resolution_rate: 0.5,
          handoff_rate: 0.25,
          avg_first_response_seconds: 30,
          avg_response_seconds: 90,
          satisfaction_avg: 4.5,
          satisfaction_count: 4,
          total_tokens: 1000,
          unattributed_tokens: 0,
          channels: [{ channel: 'widget', conversation_count: 12, share: 1 }],
        },
      });
    }
    if (path === '/tenant/analytics/timeseries') {
      return json(route, {
        data: {
          range: { from: '2026-11-15', to: '2026-11-16' },
          channel: null,
          days: [
            {
              date: '2026-11-15',
              conversation_volume: 5,
              ai_resolved: 3,
              handed_off: 1,
              satisfaction_avg: null,
              satisfaction_count: 0,
              total_tokens: 10,
            },
            {
              date: '2026-11-16',
              conversation_volume: 7,
              ai_resolved: 4,
              handed_off: 2,
              satisfaction_avg: null,
              satisfaction_count: 0,
              total_tokens: 20,
            },
          ],
        },
      });
    }
    if (path === '/tenant/conversations') {
      return json(route, {
        data: [conversation],
        pagination: { next_cursor: null, has_more: false },
      });
    }
    if (path === '/tenant/conversations/conv-1') {
      return json(route, {
        data: {
          ...conversation,
          participants: [{ type: 'customer', id: 'cust-1', display_name: 'Maya Chen' }],
        },
      });
    }
    if (path === '/tenant/conversations/conv-1/messages') {
      return json(route, {
        data: [message],
        pagination: { next_cursor: null, has_more: false },
      });
    }
    if (path === '/tenant/feedback/summary')
      return json(route, { data: { count: 0, average: null } });
    if (path === '/tenant/members') return json(route, page1([member]));
    if (path === '/tenant/knowledge/items') return json(route, page1([knowledgeItem]));
    if (path === '/tenant/knowledge/categories') return json(route, []);
    return json(route, page1([]));
  });
}

interface Target {
  readonly name: string;
  readonly url: string;
  readonly identity: keyof typeof identities;
  /** Text that is on the page once it has loaded, so axe does not look at a loading screen. */
  readonly ready: string | RegExp;
}

const TARGETS: readonly Target[] = [
  { name: 'overview', url: '/tenant/overview', identity: 'tenant', ready: 'Recent activity' },
  { name: 'inbox', url: '/tenant/conversations', identity: 'tenant', ready: 'Maya Chen' },
  {
    name: 'conversation',
    url: '/tenant/conversations/conv-1',
    identity: 'tenant',
    ready: 'Where is my order?',
  },
  {
    name: 'knowledge base',
    url: '/tenant/knowledge-base',
    identity: 'tenant',
    ready: 'Returns and exchanges',
  },
  { name: 'team', url: '/tenant/team', identity: 'tenant', ready: 'Alice Agent' },
  {
    name: 'platform tenants',
    url: '/platform/tenants',
    identity: 'platform',
    ready: 'Acme Support',
  },
];

/** WCAG 2.2 AA, and the best practices axe marks as serious. */
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

/**
 * Known violations, each with the issue that fixes it: `page`, the axe `rule` id and the Linear
 * `issue`. Empty when nothing is known.
 */
const KNOWN: readonly { page: string; theme?: string; rule: string; issue: string }[] = [];

for (const theme of ['light', 'dark'] as const) {
  test.describe(`${theme} theme`, () => {
    for (const target of TARGETS) {
      test(`${target.name} has no serious or critical accessibility violation`, async ({
        page,
      }, testInfo) => {
        await mockApi(page, identities[target.identity]);
        await page.addInitScript((mode) => localStorage.setItem('app.themeMode', mode), theme);
        await page.goto(target.url);
        await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
        await expect(page.getByText(target.ready).first()).toBeVisible();
        await expect(page.locator('app-loading-state')).toHaveCount(0);

        const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
        const serious = results.violations.filter(
          (v) => v.impact === 'serious' || v.impact === 'critical',
        );
        const unknown = serious.filter(
          (v) =>
            !KNOWN.some(
              (k) =>
                k.page === target.name &&
                (k.theme === undefined || k.theme === theme) &&
                k.rule === v.id,
            ),
        );
        if (unknown.length) {
          await testInfo.attach('axe-violations', {
            body: JSON.stringify(unknown, null, 2),
            contentType: 'application/json',
          });
        }
        expect(
          unknown.map((v) => ({
            rule: v.id,
            impact: v.impact,
            help: v.help,
            targets: v.nodes
              .slice(0, 4)
              .map((n) => `${n.target.join(' ')}  ${n.html.slice(0, 140)}`),
          })),
        ).toEqual([]);
      });
    }
  });
}

// ── Reduced motion (MOH-74) ──────────────────────────────────────────────────────────────────

/** Elements whose transition or animation lasts longer than a blink, with what runs on them. */
async function longMotion(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const seconds = (list: string) =>
      list.split(',').map((v) => (v.trim().endsWith('ms') ? parseFloat(v) / 1000 : parseFloat(v)));
    const found: string[] = [];
    for (const el of Array.from(document.querySelectorAll('*'))) {
      for (const pseudo of [null, '::before', '::after']) {
        const style = getComputedStyle(el, pseudo);
        const transition = Math.max(...seconds(style.transitionDuration));
        const animation =
          style.animationName === 'none' ? 0 : Math.max(...seconds(style.animationDuration));
        if (transition > 0.05 || animation > 0.05) {
          found.push(
            `${el.tagName.toLowerCase()}${pseudo ?? ''}.${String(el.getAttribute('class')).split(' ')[0]}`,
          );
        }
      }
    }
    return found.slice(0, 8);
  });
}

test.describe('motion', () => {
  test('the chat widget has no long transition for a visitor who asked for less motion', async ({
    page,
  }) => {
    await installWidgetHarness(page);
    await page.goto(HOST_URL);
    await page.locator('button[aria-label="Open chat"]').click();
    const frame = page.frameLocator('iframe[title="Chat widget"]');
    const send = frame.locator('button[aria-label="Send message"]');
    await expect(send).toBeVisible();
    const duration = () =>
      send.evaluate((el) => parseFloat(getComputedStyle(el).transitionDuration));

    // The send button fades when it is disabled: it takes a moment, unless motion is reduced.
    expect(await duration()).toBeGreaterThan(0.05);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    expect(await duration()).toBeLessThan(0.001);
  });

  test('the dashboard moves normally, and not at all for a person who asked for less motion', async ({
    page,
  }) => {
    await mockApi(page, identities.tenant, { hold: ['/tenant/conversations'] });
    await page.goto('/tenant/conversations');
    await expect(page.locator('app-loading-state')).toBeVisible();
    // Without the preference something moves (the spinner), so the check below is not vacuous.
    expect(await longMotion(page)).not.toEqual([]);

    await page.emulateMedia({ reducedMotion: 'reduce' });
    expect(await longMotion(page)).toEqual([]);
    const spinner = await page.locator('app-loading-state span').evaluate((el) => ({
      duration: getComputedStyle(el).animationDuration,
      count: getComputedStyle(el).animationIterationCount,
    }));
    // Browsers write 0.01ms as `1e-05s`: compare the number, not the text.
    expect(parseFloat(spinner.duration)).toBeLessThan(0.001);
    expect(spinner.count).toBe('1');
  });

  test('the shell has no long transition either, with the sidebar drawer and menus', async ({
    page,
  }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await mockApi(page, identities.tenant);
    await page.goto('/tenant/overview');
    await expect(page.getByText('Recent activity').first()).toBeVisible();
    await page.getByRole('button', { name: 'User menu' }).click();
    await expect(page.getByRole('menu')).toBeVisible();
    expect(await longMotion(page)).toEqual([]);
  });
});

// ── Loading, empty and page-container states in both themes (spec 009 T073) ─────────────────

interface Paint {
  readonly ratio: number;
  readonly fg: string;
  readonly bg: string;
}

/**
 * The contrast between the text colour of the first element matching `selector` and the colour
 * it really sits on: every background above it, blended from the page down.
 */
async function paint(page: Page, selector: string): Promise<Paint> {
  return page
    .locator(selector)
    .first()
    .evaluate((el) => {
      const rgba = (colour: string): [number, number, number, number] => {
        const ctx = document.createElement('canvas').getContext('2d')!;
        ctx.clearRect(0, 0, 1, 1);
        ctx.fillStyle = colour;
        ctx.fillRect(0, 0, 1, 1);
        const d = ctx.getImageData(0, 0, 1, 1).data;
        return [d[0], d[1], d[2], d[3] / 255];
      };
      const layers: string[] = [];
      for (let node: Element | null = el; node; node = node.parentElement) {
        layers.unshift(getComputedStyle(node).backgroundColor);
      }
      let base: [number, number, number] = [255, 255, 255];
      for (const layer of layers) {
        const [r, g, b, a] = rgba(layer);
        base = [r * a + base[0] * (1 - a), g * a + base[1] * (1 - a), b * a + base[2] * (1 - a)];
      }
      const [fr, fg, fb, fa] = rgba(getComputedStyle(el).color);
      const text: [number, number, number] = [
        fr * fa + base[0] * (1 - fa),
        fg * fa + base[1] * (1 - fa),
        fb * fa + base[2] * (1 - fa),
      ];
      const luminance = ([r, g, b]: [number, number, number]) => {
        const f = (c: number) => {
          const v = c / 255;
          return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
        };
        return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
      };
      const [hi, lo] = [luminance(text), luminance(base)].sort((x, y) => y - x);
      const hex = (c: [number, number, number]) =>
        c.map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
      return { ratio: (hi + 0.05) / (lo + 0.05), fg: hex(text), bg: hex(base) };
    });
}

const STATES = [
  {
    name: 'loading',
    options: { hold: ['/tenant/conversations'] } as MockOptions,
    url: '/tenant/conversations',
    wait: 'app-loading-state',
    texts: ['app-loading-state p'],
  },
  {
    name: 'empty',
    options: { emptyInbox: true } as MockOptions,
    url: '/tenant/conversations',
    wait: 'app-empty-state',
    texts: ['app-empty-state h2', 'app-empty-state p'],
  },
  {
    name: 'page container',
    options: {} as MockOptions,
    url: '/tenant/overview',
    wait: 'app-page-container',
    texts: [
      'app-page-container h1',
      'app-page-container app-dashboard-card h3, app-page-container app-dashboard-card p',
    ],
  },
];

for (const state of STATES) {
  test.describe(`${state.name} state`, () => {
    const seen: Record<string, string> = {};
    for (const theme of ['light', 'dark'] as const) {
      test(`is legible in the ${theme} theme`, async ({ page }) => {
        await mockApi(page, identities.tenant, state.options);
        await page.addInitScript((mode) => localStorage.setItem('app.themeMode', mode), theme);
        await page.goto(state.url);
        await expect(page.locator(state.wait).first()).toBeVisible();
        for (const selector of state.texts) {
          if ((await page.locator(selector).count()) === 0) continue;
          const result = await paint(page, selector);
          expect(
            result.ratio,
            `${selector}: #${result.fg} on #${result.bg}`,
          ).toBeGreaterThanOrEqual(4.5);
          seen[`${theme}:${selector}`] = `${result.fg}/${result.bg}`;
        }
        expect(Object.keys(seen).some((k) => k.startsWith(`${theme}:`))).toBe(true);
      });
    }

    test('follows the live system theme while it is shown', async ({ page }) => {
      await page.emulateMedia({ colorScheme: 'dark' });
      await mockApi(page, identities.tenant, state.options);
      await page.goto(state.url);
      await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
      await expect(page.locator(state.wait).first()).toBeVisible();
      const selector = state.texts.find(Boolean)!;
      const dark = await paint(page, selector);

      await page.emulateMedia({ colorScheme: 'light' });
      await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
      await expect(page.locator(state.wait).first()).toBeVisible();
      const light = await paint(page, selector);

      // The same element, still on screen, changed its colours with the theme and stayed legible.
      expect(light.bg).not.toBe(dark.bg);
      expect(light.fg).not.toBe(dark.fg);
      expect(light.ratio).toBeGreaterThanOrEqual(4.5);
      expect(dark.ratio).toBeGreaterThanOrEqual(4.5);
    });
  });
}
