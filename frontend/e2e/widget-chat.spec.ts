import { test, expect } from '@playwright/test';

test.describe('Widget Chat (US1)', () => {
  test('full US1 journey: open launcher → welcome message → send → bubble → reply', async ({
    page,
  }) => {
    await page.goto('/fixtures/widget-host.html');

    const launcher = page.locator('button[aria-label="Open chat"]');
    await expect(launcher).toBeVisible({ timeout: 10000 });

    await launcher.click();

    const iframe = page.frameLocator('iframe[title="Chat widget"]');
    await expect(iframe.locator('text=Hi! How can we help?')).toBeVisible({
      timeout: 10000,
    });

    const textarea = iframe.locator('textarea');
    await textarea.fill('Hello');

    const sendBtn = iframe.locator('button[aria-label="Send message"]');
    await sendBtn.click();

    await expect(iframe.locator('text=Hello')).toBeVisible({ timeout: 5000 });

    const typing = iframe.locator('[role="status"]');
    await expect(typing).toBeVisible({ timeout: 2000 });
  });
});
