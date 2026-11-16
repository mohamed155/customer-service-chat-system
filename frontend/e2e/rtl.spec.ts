import { Page, Route, TestInfo } from '@playwright/test';
import { expect, test } from './support/test';
import { installWidgetHarness, HOST_URL, WIDGET_ORIGIN } from './support/widget-harness';

/**
 * Right-to-left layout (MOH-73). The language picks the direction (`dir` on the document) and the
 * layout, written with logical properties, follows. Each check runs in English and in Arabic and
 * compares positions, so a layout that is always flipped, or never flipped, fails one of the two.
 * Screenshots go to the test report (they are evidence for a reviewer, not compared against a
 * baseline: a baseline image differs between operating systems and fonts).
 */

const PERMISSIONS = [
  'overview.view',
  'conversations.view',
  'conversations.manage',
  'customers.view',
];

const identity = {
  id: 'user-1',
  email: 'owner@acme.test',
  displayName: 'Olivia Owner',
  platformRole: null,
  platformPermissions: [],
  staffTenantPermissions: null,
  memberships: [
    {
      tenantId: 'tenant-1',
      tenantName: 'Acme Support',
      tenantSlug: 'acme-support',
      role: 'owner',
      permissions: PERMISSIONS,
    },
  ],
};

const conversation = {
  id: 'conv-1',
  customer: { id: 'cust-1', display_name: 'Maya Chen' },
  channel: 'web_chat',
  status: 'open',
  assignee: null,
  last_message: { kind: 'customer', preview: 'Where is my order?' },
  last_activity_at: '2026-11-16T09:00:00Z',
  created_at: '2026-11-16T08:00:00Z',
};

const arabic = {
  sidebar: { item: { overview: 'نظرة عامة', conversations: 'المحادثات' } },
  pages: { overview: { title: 'نظرة عامة', subtitle: '{date} · مركز الدعم' } },
};

function json(route: Route, body: unknown) {
  return route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify(body),
  });
}

async function setUp(page: Page, locale: 'en' | 'ar') {
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    if (path === '/me') return json(route, identity);
    if (path === '/tenant/conversations') {
      return json(route, {
        data: [conversation],
        pagination: { next_cursor: null, has_more: false },
      });
    }
    if (path === '/tenant/feedback/summary')
      return json(route, { data: { count: 0, average: null } });
    return json(route, { items: [], nextCursor: null });
  });
  await page.route('**/i18n/shared/ar.json', (route) => json(route, arabic));
  await page.addInitScript((tag) => localStorage.setItem('app.locale', tag), locale);
}

const box = async (locator: ReturnType<Page['locator']>) => {
  const rect = await locator.boundingBox();
  if (!rect) throw new Error('not on the page');
  return rect;
};

async function evidence(page: Page, testInfo: TestInfo, name: string) {
  await testInfo.attach(name, { body: await page.screenshot(), contentType: 'image/png' });
}

for (const locale of ['en', 'ar'] as const) {
  const rtl = locale === 'ar';
  // `a` comes before `b` in the reading direction: further left in English, further right in Arabic.
  const before = (a: number, b: number) => (rtl ? a > b : a < b);

  test.describe(`${rtl ? 'right to left (Arabic)' : 'left to right (English)'}`, () => {
    test('the document follows the language', async ({ page }) => {
      await setUp(page, locale);
      await page.goto('/tenant/overview');
      await expect(page.locator('html')).toHaveAttribute('dir', rtl ? 'rtl' : 'ltr');
      await expect(page.locator('html')).toHaveAttribute('lang', locale);
    });

    test('arrows point the way they go, and a directional icon is mirrored', async ({ page }) => {
      await setUp(page, locale);
      await page.goto('/tenant/overview');
      await expect(page.locator('html')).toHaveAttribute('dir', rtl ? 'rtl' : 'ltr');

      const seen = await page.evaluate(() => {
        const make = (cls: string) => {
          const el = document.createElement('span');
          el.className = cls;
          el.textContent = 'x';
          document.body.appendChild(el);
          return el;
        };
        const back = make('app-arrow-back');
        const forward = make('app-arrow-forward');
        const icon = make('app-flip-rtl');
        return {
          back: getComputedStyle(back, '::before').content,
          forward: getComputedStyle(forward, '::after').content,
          flip: getComputedStyle(icon).transform,
        };
      });
      // "Back" points to where you came from: left in English, right in Arabic; "forward" the reverse.
      expect(seen.back).toContain(rtl ? '→' : '←');
      expect(seen.forward).toContain(rtl ? '←' : '→');
      expect(seen.flip).toBe(rtl ? 'matrix(-1, 0, 0, 1, 0, 0)' : 'none');
    });

    test('the shell mirrors: sidebar, content and top bar swap sides', async ({
      page,
    }, testInfo) => {
      await setUp(page, locale);
      await page.goto('/tenant/overview');
      const nav = page.getByRole('navigation', { name: 'Primary navigation' });
      await expect(nav).toBeVisible();

      const sidebar = await box(nav);
      const content = await box(page.locator('main, [role="main"]').first());
      // The sidebar starts the page: on the left in English, on the right in Arabic.
      expect(before(sidebar.x, content.x)).toBe(true);
      const title = await box(page.locator('header .title'));
      const tools = await box(page.locator('header .tools'));
      expect(before(title.x, tools.x)).toBe(true);
      await evidence(page, testInfo, `shell-${locale}`);
    });

    test('the inbox mirrors: avatar first, time last in the reading direction', async ({
      page,
    }, testInfo) => {
      await setUp(page, locale);
      await page.goto('/tenant/conversations');
      const item = page.locator('app-inbox-list .item').first();
      await expect(item).toBeVisible();

      const avatar = await box(item.locator('app-avatar'));
      const time = await box(item.locator('time'));
      expect(before(avatar.x, time.x)).toBe(true);
      // The list's divider is on its end side, and the title still reads from the start.
      const border = await page.locator('app-inbox-list').evaluate((el) => {
        const style = getComputedStyle(el);
        return { start: style.borderInlineStartWidth, end: style.borderInlineEndWidth };
      });
      expect(parseFloat(border.end)).toBeGreaterThanOrEqual(parseFloat(border.start));
      await evidence(page, testInfo, `inbox-${locale}`);
    });

    test('a form mirrors: the dialog opens centred, fields run with the direction, buttons swap', async ({
      page,
    }, testInfo) => {
      await setUp(page, locale);
      await page.goto('/tenant/conversations');
      await page.locator('.new-btn').click();
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();

      // Centred on the page in both directions (the centring maths follows the direction).
      const panel = await box(dialog);
      const viewport = page.viewportSize()!;
      expect(Math.abs(panel.x + panel.width / 2 - viewport.width / 2)).toBeLessThan(2);

      const direction = await dialog
        .locator('input[type="text"]')
        .first()
        .evaluate((el) => getComputedStyle(el).direction);
      expect(direction).toBe(rtl ? 'rtl' : 'ltr');

      // Cancel comes before Create in the reading order: left of it in English, right of it in Arabic.
      const cancel = await box(dialog.getByRole('button', { name: 'Cancel' }));
      const create = await box(dialog.getByRole('button', { name: 'Create conversation' }));
      expect(before(cancel.x, create.x)).toBe(true);
      // The buttons sit at the end of the dialog: right in English, left in Arabic.
      const dialogCentre = panel.x + panel.width / 2;
      expect(before(dialogCentre, create.x)).toBe(true);
      await evidence(page, testInfo, `form-${locale}`);
    });
  });
}

test.describe('the chat widget', () => {
  for (const locale of ['en', 'ar'] as const) {
    const rtl = locale === 'ar';
    test.describe(`in ${locale}`, () => {
      test.use({ locale });

      test(`mirrors: the visitor's message is on the ${rtl ? 'left' : 'right'}`, async ({
        page,
      }, testInfo) => {
        await installWidgetHarness(page);
        // The harness serves the widget's English file only; Arabic is a translation file next to it.
        await page.route('**/i18n/ar.json', (route) =>
          route.fulfill({
            status: 200,
            contentType: 'application/json',
            headers: { 'access-control-allow-origin': '*' },
            body: JSON.stringify({ 'composer.send': 'إرسال', 'chat.close': 'إغلاق' }),
          }),
        );
        await page.goto(HOST_URL);

        const launcher = page.locator('button[aria-label="Open chat"]');
        await expect(launcher).toBeVisible({ timeout: 10000 });
        await launcher.click();

        const frame = page.frameLocator('iframe[title="Chat widget"]');
        await expect(frame.locator('html')).toHaveAttribute('dir', rtl ? 'rtl' : 'ltr');

        // The greeting is the assistant's side; the visitor's own message takes the other.
        const greeting = frame.locator('.window__welcome .bubble--assistant');
        await expect(greeting).toBeVisible();
        const greetingBox = await box(greeting);

        await frame.locator('textarea').fill('Hello');
        await frame.locator('textarea').press('Enter');
        const visitor = frame.locator('.bubble--visitor').first();
        await expect(visitor).toBeVisible();
        const visitorBox = await box(visitor);

        // The visitor's side is the end of the line: right in English, left in Arabic.
        const centre = (b: { x: number; width: number }) => b.x + b.width / 2;
        expect(centre(visitorBox) > centre(greetingBox)).toBe(!rtl);
        await evidence(page, testInfo, `widget-${locale}`);
      });
    });
  }
});
