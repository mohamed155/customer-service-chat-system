import { chromium } from 'playwright';

const browser = await chromium.launch();
const page = await browser.newPage();

await page.addInitScript(() => {
  localStorage.setItem(
    'app.tenant',
    JSON.stringify({
      id: 'tenant-1',
      name: 'Acme Support',
      slug: 'acme-support',
      status: 'active',
    }),
  );
});

await page.route('**/api/v1/**', async (route) => {
  const url = new URL(route.request().url());
  const path = url.pathname.replace('/api/v1', '');
  if (path === '/me') {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
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
            permissions: [
              'overview.view',
              'conversations.view',
              'customers.view',
              'ai_agent.view',
              'knowledge_base.view',
              'integrations.view',
              'analytics.view',
              'settings.view',
            ],
          },
        ],
      }),
    });
    return;
  }
  await route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ items: [], nextCursor: null }),
  });
});

await page.goto('http://127.0.0.1:4201/tenant/overview', {
  waitUntil: 'networkidle',
  timeout: 15000,
});

const url = page.url();
const sidebar = await page.evaluate(() => !!document.querySelector('app-sidebar'));
const nav = await page.evaluate(
  () => !!document.querySelector('nav[aria-label="Primary navigation"]'),
);
const topbar = await page.evaluate(() => !!document.querySelector('app-topbar'));
const heading = await page.evaluate(
  () => document.querySelector('h1, h2, h3')?.textContent || null,
);
const bodyText = await page.evaluate(() => document.body.innerText.substring(0, 500));
const htmlTheme = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));

console.log('URL:', url);
console.log('html theme:', htmlTheme);
console.log('has app-sidebar:', sidebar);
console.log('has primary nav:', nav);
console.log('has app-topbar:', topbar);
console.log('heading:', heading);
console.log('body text snippet:', bodyText);

await browser.close();
