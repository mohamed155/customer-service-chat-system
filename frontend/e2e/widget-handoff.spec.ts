import { test, expect } from '@playwright/test';

test.describe('Widget Handoff (US3)', () => {
  test('handoff banner shows when conversation is handed to human', async ({ page }) => {
    await page.goto('/fixtures/widget-host.html');

    const launcher = page.locator('button[aria-label="Open chat"]');
    await expect(launcher).toBeVisible({ timeout: 10000 });
    await launcher.click();

    const iframe = page.frameLocator('iframe[title="Chat widget"]');

    const banner = iframe.locator('text=Connecting you to a support agent');
    await expect(banner).toBeVisible({ timeout: 10000 });
  });
});
