import { Page, Route } from '@playwright/test';
import { expect, test } from './support/test';

// Languages in the dashboard (MOH-155, ADR-016). The static server sends the production policy
// (script-src 'self' without 'unsafe-eval', Trusted Types), so these specs also prove that ICU
// messages and catalog loading work under it: `support/test.ts` fails any violation.

const identity = {
  id: 'tenant-user',
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
      permissions: ['overview.view', 'conversations.view', 'settings.view'],
    },
  ],
};

async function json(route: Route, data: unknown, status = 200) {
  await route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

async function mockApi(page: Page) {
  await page.route('**/api/v1/**', async (route) => {
    const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
    if (path === '/me') return json(route, identity);
    return json(route, { items: [], nextCursor: null });
  });
}

/** The requests for catalog files, in order. */
function watchCatalogs(page: Page): string[] {
  const asked: string[] = [];
  page.on('request', (request) => {
    const { pathname } = new URL(request.url());
    if (pathname.startsWith('/i18n/')) asked.push(pathname);
  });
  return asked;
}

const german = {
  app: { brandName: 'Helix', tagline: 'Support-KI' },
  sidebar: {
    item: { overview: 'Übersicht', conversations: 'Gespräche', settings: 'Einstellungen' },
  },
  pages: { overview: { title: 'Übersicht', subtitle: '{date} · Ihr Support-Cockpit' } },
};

test.describe('languages', () => {
  test('English needs no catalog request and fills the date into a message', async ({ page }) => {
    const asked = watchCatalogs(page);
    await mockApi(page);
    await page.goto('/tenant/overview');
    const nav = page.getByRole('navigation', { name: 'Primary navigation' });
    await expect(nav).toContainText('Conversations');
    // The overview subtitle is the ICU message "{date} · Your support cockpit".
    await expect(page.locator('header .title span')).toHaveText(
      /\w+day, \w+ \d+ · Your support cockpit/,
    );
    await expect(page.locator('html')).toHaveAttribute('lang', 'en');
    expect(asked).toEqual([]);
  });

  test('a stored language is loaded from its catalog file and used, dates included', async ({
    page,
  }) => {
    const asked = watchCatalogs(page);
    await mockApi(page);
    await page.route('**/i18n/shared/de.json', (route) => json(route, german));
    await page.addInitScript(() => localStorage.setItem('app.locale', 'de'));
    await page.goto('/tenant/overview');

    const nav = page.getByRole('navigation', { name: 'Primary navigation' });
    await expect(nav).toContainText('Gespräche');
    await expect(nav).toContainText('Einstellungen');
    await expect(page.locator('header .title span')).toHaveText(
      /(Montag|Dienstag|Mittwoch|Donnerstag|Freitag|Samstag|Sonntag), \d+\. \w+ · Ihr Support-Cockpit/,
    );
    // A key German lacks falls back to English: nothing is blank.
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Account security',
    );
    await expect(page.locator('html')).toHaveAttribute('lang', 'de');
    // The overview is an area with its own scope: German has no file for it, which is not a
    // failure. The area shows its English, the rest of the page stays German.
    await expect(page.getByText('Conversation trends')).toBeVisible();
    expect(asked).toEqual(['/i18n/shared/de.json', '/i18n/insights/de.json']);
  });

  test("the browser's language is used when it has a catalog", async ({ browser }) => {
    const context = await browser.newContext({ locale: 'de-AT' });
    const page = await context.newPage();
    await mockApi(page);
    await page.route('**/i18n/shared/de-AT.json', (route) => route.fulfill({ status: 404 }));
    await page.route('**/i18n/shared/de.json', (route) => json(route, german));
    await page.goto('/tenant/overview');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Gespräche',
    );
    await context.close();
  });

  test('a language with no catalog file shows English', async ({ browser }) => {
    const context = await browser.newContext({ locale: 'ja-JP' });
    const page = await context.newPage();
    const asked = watchCatalogs(page);
    await mockApi(page);
    await page.goto('/tenant/overview');
    await expect(page.getByRole('navigation', { name: 'Primary navigation' })).toContainText(
      'Conversations',
    );
    await expect(page.locator('html')).toHaveAttribute('lang', 'en');
    // The real server answered 404 for each file it was asked for, and the page went on in English.
    expect(asked).toEqual(['/i18n/shared/ja-JP.json', '/i18n/shared/ja.json']);
    await context.close();
  });
});
