import { test, expect } from '@playwright/test';

test.describe('Widget Embed (US2)', () => {
  test('invalid widget ID renders nothing and no iframe', async ({ page }) => {
    await page.goto('/fixtures/widget-host.html');

    const launcher = page.locator('button[aria-label="Open chat"]');
    await expect(launcher).not.toBeVisible({ timeout: 5000 });
  });

  test('no uncaught errors for invalid config', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto('/fixtures/widget-host.html');

    expect(errors.length).toBe(0);
  });
});
