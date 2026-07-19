import { expect, Page, Route, test } from '@playwright/test';

const testTenant = {
  id: 'tenant-an',
  name: 'Analytics Co',
  slug: 'analytics-co',
  status: 'active' as const,
  plan: 'professional' as const,
};

function adminIdentity() {
  return {
    id: 'user-admin',
    email: 'admin@test.com',
    displayName: 'Admin User',
    platformRole: null,
    platformPermissions: [],
    staffTenantPermissions: null,
    memberships: [
      {
        tenantId: 'tenant-an',
        tenantName: 'Analytics Co',
        tenantSlug: 'analytics-co',
        role: 'admin' as const,
        permissions: ['analytics.view', 'conversations.view', 'members.view'],
      },
    ],
  };
}

const SUMMARY_FIXTURE = {
  data: {
    range: { from: '2026-06-20', to: '2026-07-19' },
    channel: null,
    conversation_volume: 1240,
    concluded_count: 1100,
    ai_resolution_rate: 0.78,
    handoff_rate: 0.19,
    avg_first_response_seconds: 4.2,
    avg_response_seconds: 6.8,
    satisfaction_avg: 4.3,
    satisfaction_count: 312,
    total_tokens: 5482210,
    unattributed_tokens: 120400,
    channels: [
      { channel: 'widget', conversation_count: 1180, share: 0.952 },
      { channel: 'email', conversation_count: 60, share: 0.048 },
    ],
  },
};

const TIMESERIES_FIXTURE = {
  data: {
    range: { from: '2026-07-13', to: '2026-07-19' },
    channel: null,
    days: [
      {
        date: '2026-07-13',
        conversation_volume: 40,
        ai_resolved: 28,
        handed_off: 6,
        satisfaction_avg: 4.5,
        satisfaction_count: 11,
        total_tokens: 182000,
      },
      {
        date: '2026-07-14',
        conversation_volume: 35,
        ai_resolved: 25,
        handed_off: 5,
        satisfaction_avg: 4.2,
        satisfaction_count: 9,
        total_tokens: 160000,
      },
      {
        date: '2026-07-15',
        conversation_volume: 50,
        ai_resolved: 38,
        handed_off: 8,
        satisfaction_avg: 4.6,
        satisfaction_count: 14,
        total_tokens: 220000,
      },
      {
        date: '2026-07-16',
        conversation_volume: 42,
        ai_resolved: 30,
        handed_off: 7,
        satisfaction_avg: 4.4,
        satisfaction_count: 10,
        total_tokens: 195000,
      },
      {
        date: '2026-07-17',
        conversation_volume: 38,
        ai_resolved: 27,
        handed_off: 6,
        satisfaction_avg: 4.3,
        satisfaction_count: 12,
        total_tokens: 175000,
      },
      {
        date: '2026-07-18',
        conversation_volume: 55,
        ai_resolved: 40,
        handed_off: 10,
        satisfaction_avg: null,
        satisfaction_count: 0,
        total_tokens: 250000,
      },
      {
        date: '2026-07-19',
        conversation_volume: 45,
        ai_resolved: 32,
        handed_off: 8,
        satisfaction_avg: 4.1,
        satisfaction_count: 8,
        total_tokens: 200000,
      },
    ],
  },
};

const EMPTY_SUMMARY_FIXTURE = {
  data: {
    range: { from: '2026-07-19', to: '2026-07-19' },
    channel: null,
    conversation_volume: 0,
    concluded_count: 0,
    ai_resolution_rate: null,
    handoff_rate: null,
    avg_first_response_seconds: null,
    avg_response_seconds: null,
    satisfaction_avg: null,
    satisfaction_count: 0,
    total_tokens: 0,
    unattributed_tokens: 0,
    channels: [],
  },
};

const EMPTY_TIMESERIES_FIXTURE = {
  data: {
    range: { from: '2026-07-19', to: '2026-07-19' },
    channel: null,
    days: [
      {
        date: '2026-07-19',
        conversation_volume: 0,
        ai_resolved: 0,
        handed_off: 0,
        satisfaction_avg: null,
        satisfaction_count: 0,
        total_tokens: 0,
      },
    ],
  },
};

function json(route: Route, data: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) });
}

function queryParams(url: URL): Record<string, string> {
  const params: Record<string, string> = {};
  url.searchParams.forEach((v, k) => {
    params[k] = v;
  });
  return params;
}

async function installAnalyticsApi(page: Page) {
  await page.route('**/api/v1/**', async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname.replace('/api/v1', '');

    if (path === '/me') return json(route, adminIdentity());

    if (path === '/tenant/analytics/summary') {
      const params = queryParams(url);
      return json(route, {
        ...SUMMARY_FIXTURE,
        data: {
          ...SUMMARY_FIXTURE.data,
          range: {
            from: params['from'] ?? SUMMARY_FIXTURE.data.range.from,
            to: params['to'] ?? SUMMARY_FIXTURE.data.range.to,
          },
          channel: params['channel'] ?? null,
        },
      });
    }

    if (path === '/tenant/analytics/timeseries') {
      const params = queryParams(url);
      return json(route, {
        ...TIMESERIES_FIXTURE,
        data: {
          ...TIMESERIES_FIXTURE.data,
          range: {
            from: params['from'] ?? TIMESERIES_FIXTURE.data.range.from,
            to: params['to'] ?? TIMESERIES_FIXTURE.data.range.to,
          },
          channel: params['channel'] ?? null,
        },
      });
    }

    return json(route, { data: null });
  });
}

test.describe('Analytics', () => {
  test('Cards render: seven metric cards with fixture values', async ({ page }) => {
    await installAnalyticsApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/analytics');

    const cards = page.locator('app-metric-card');
    await expect(cards).toHaveCount(7);

    await expect(cards.nth(0)).toContainText('1240');
    await expect(cards.nth(1)).toContainText('78.0%');
    await expect(cards.nth(2)).toContainText('19.0%');
    await expect(cards.nth(3)).toContainText('4s');
    await expect(cards.nth(4)).toContainText('7s');
    await expect(cards.nth(5)).toContainText('4.3 / 5 (312 ratings)');
    await expect(cards.nth(6)).toContainText((5482210).toLocaleString());
  });

  test('Date preset drives the request: selecting "Last 7 days" issues summary request with 7-day range', async ({
    page,
  }) => {
    const summaryRequests: string[] = [];
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');

      if (path === '/me') return json(route, adminIdentity());
      if (path === '/tenant/analytics/summary') {
        summaryRequests.push(route.request().url());
        return json(route, SUMMARY_FIXTURE);
      }
      if (path === '/tenant/analytics/timeseries') {
        return json(route, TIMESERIES_FIXTURE);
      }
      return json(route, { data: null });
    });

    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/analytics');
    await expect(page.getByLabel('Date range')).toBeVisible();

    await page.getByLabel('Date range').selectOption('7');
    await page.waitForTimeout(500);

    expect(summaryRequests.length).toBeGreaterThanOrEqual(2);
    const lastRequest = summaryRequests[summaryRequests.length - 1];
    const searchParams = new URL(lastRequest).searchParams;
    const from = searchParams.get('from');
    const to = searchParams.get('to');
    expect(from).toBeTruthy();
    expect(to).toBeTruthy();
    const diff = new Date(to as string).getTime() - new Date(from as string).getTime();
    const days = Math.round(diff / (1000 * 60 * 60 * 24));
    expect(days).toBeLessThanOrEqual(7);
  });

  test('Channel filter drives the request: selecting website widget issues requests with channel=widget', async ({
    page,
  }) => {
    const summaryUrls: string[] = [];
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');

      if (path === '/me') return json(route, adminIdentity());
      if (path === '/tenant/analytics/summary') {
        summaryUrls.push(route.request().url());
        return json(route, SUMMARY_FIXTURE);
      }
      if (path === '/tenant/analytics/timeseries') {
        return json(route, TIMESERIES_FIXTURE);
      }
      return json(route, { data: null });
    });

    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/analytics');
    await expect(page.getByLabel('Channel', { exact: true })).toBeVisible();

    await page.getByLabel('Channel', { exact: true }).selectOption('widget');
    await page.waitForTimeout(500);

    const lastRequest = summaryUrls[summaryUrls.length - 1];
    expect(new URL(lastRequest).searchParams.get('channel')).toBe('widget');
  });

  test('Charts render: four app-trend-chart elements, handoff chart has two legend entries', async ({
    page,
  }) => {
    await installAnalyticsApi(page);
    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/analytics');

    const charts = page.locator('app-trend-chart');
    await expect(charts).toHaveCount(4);

    const handoffChart = charts.nth(1);
    const legendItems = handoffChart.locator('.legend li');
    await expect(legendItems).toHaveCount(2);
    await expect(legendItems.nth(0)).toContainText('AI resolved');
    await expect(legendItems.nth(1)).toContainText('Human handoff');
  });

  test('Empty state: zeros with null rates show em dash not 0%', async ({ page }) => {
    await page.route('**/api/v1/**', async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace('/api/v1', '');

      if (path === '/me') {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(adminIdentity()),
        });
      }
      if (path === '/tenant/analytics/summary') {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(EMPTY_SUMMARY_FIXTURE),
        });
      }
      if (path === '/tenant/analytics/timeseries') {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(EMPTY_TIMESERIES_FIXTURE),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ data: null }),
      });
    });

    await page.addInitScript((tenant) => {
      localStorage.setItem('app.tenant', JSON.stringify(tenant));
    }, testTenant);

    await page.goto('/tenant/analytics');

    const cards = page.locator('app-metric-card');
    await expect(cards).toHaveCount(7);

    await expect(cards.nth(0)).toContainText('0');
    await expect(cards.nth(1)).toContainText('\u2014');
    await expect(cards.nth(2)).toContainText('\u2014');
    await expect(cards.nth(3)).toContainText('\u2014');
    await expect(cards.nth(4)).toContainText('\u2014');
    await expect(cards.nth(5)).toContainText('\u2014');
    await expect(cards.nth(6)).toContainText('0');

    const text = await page.locator('app-analytics').innerText();
    expect(text).not.toContain('0%');
  });
});
