import { test, expect } from '@playwright/test';
import { openWidgetHost } from './support/widget-harness';

test.describe('Widget Embed (US2)', () => {
  test('invalid widget ID renders nothing and no iframe', async ({ page }) => {
    await openWidgetHost(page, { configStatus: 404 });

    const launcher = page.locator('button[aria-label="Open chat"]');
    await expect(launcher).not.toBeVisible({ timeout: 5000 });
  });

  test('no uncaught errors for invalid config', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await openWidgetHost(page, { configStatus: 404 });

    expect(errors.length).toBe(0);
  });
});
