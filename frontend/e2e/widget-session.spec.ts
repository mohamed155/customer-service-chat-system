import { test, expect } from '@playwright/test';

test.describe('Widget Session (US4)', () => {
  test('reload preserves conversation', async ({ page }) => {
    await page.goto('/fixtures/widget-host.html');

    const launcher = page.locator('button[aria-label="Open chat"]');
    await expect(launcher).toBeVisible({ timeout: 10000 });
    await launcher.click();

    const iframe = page.frameLocator('iframe[title="Chat widget"]');

    const textarea = iframe.locator('textarea');
    await textarea.fill('Hello');
    const sendBtn = iframe.locator('button[aria-label="Send message"]');
    await sendBtn.click();

    await page.reload();

    const launcher2 = page.locator('button[aria-label="Open chat"]');
    await expect(launcher2).toBeVisible({ timeout: 10000 });
    await launcher2.click();

    const iframe2 = page.frameLocator('iframe[title="Chat widget"]');
    await expect(iframe2.locator('textarea')).toBeVisible({ timeout: 10000 });
  });
});
