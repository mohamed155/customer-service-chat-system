import { test, expect, Page } from '@playwright/test';
import { openWidgetHost, WIDGET_ORIGIN } from './support/widget-harness';

const ACCEPT = 'application/problem+json, application/json;q=0.5';

/** An RFC 9457 problem document, as the API sends it to a client that asks for one. */
const problem = (status: number, code: string, detail: string) =>
  JSON.stringify({
    type: `urn:problem:${code}`,
    title: detail,
    status,
    detail,
    instance: '/widget/v1/conversations/c1/messages',
    code,
    request_id: 'req_e2e',
    errors: [],
  });

/** Opens the widget and sends a message the API refuses with `status` and `body`. */
async function sendRefused(page: Page, status: number, body: string, contentType: string) {
  await openWidgetHost(page);
  const accepts: (string | undefined)[] = [];
  // Registered after the harness's own routes, so it answers first.
  await page.context().route(`${WIDGET_ORIGIN}/widget/v1/conversations/*/messages`, (route) => {
    accepts.push(route.request().headers()['accept']);
    return route.fulfill({ status, contentType, body });
  });

  await page.locator('button[aria-label="Open chat"]').click({ timeout: 10000 });
  const iframe = page.frameLocator('iframe[title="Chat widget"]');
  await iframe.locator('textarea').fill('Hello');
  await iframe.locator('button[aria-label="Send message"]').click();
  return { iframe, accepts };
}

test.describe('Widget errors (MOH-200)', () => {
  test('asks for problem+json and shows the generic error for a problem 500', async ({ page }) => {
    const { iframe, accepts } = await sendRefused(
      page,
      500,
      problem(500, 'internal_error', 'Internal server error'),
      'application/problem+json',
    );
    await expect(iframe.getByText('Something went wrong.')).toBeVisible({ timeout: 5000 });
    expect(accepts).toEqual([ACCEPT]);
  });

  test('shows the same error for the legacy envelope and for a gateway page', async ({ page }) => {
    for (const [body, contentType] of [
      [
        JSON.stringify({
          error: { code: 'internal_error', message: 'x', details: [], request_id: 'r' },
        }),
        'application/json',
      ],
      ['<html>Bad gateway</html>', 'text/html'],
    ]) {
      const { iframe } = await sendRefused(page, 502, body, contentType);
      await expect(iframe.getByText('Something went wrong.')).toBeVisible({ timeout: 5000 });
    }
  });
});
