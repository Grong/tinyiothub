import { expect, test } from '@playwright/test';

test('assistant workbench renders drawer, timeline, hud and composer', async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('pageerror', (error) => consoleErrors.push(error.message));

  await page.goto('/');
  await page.evaluate(() => {
    localStorage.setItem('auth-token', 'test-token');
    localStorage.setItem('workspace-id', 'ws-e2e');
  });
  // app.ts routes on pathname (handleRoute), so the chat route is `/chat`.
  await page.goto('/chat');
  await expect(page.locator('view-chat session-drawer')).toBeVisible();
  await expect(page.locator('view-chat global-hud')).toBeVisible();
  await expect(page.locator('view-chat .chat-input')).toBeVisible();
  await expect(page.locator('view-chat .codex-thread')).toBeVisible();
  // Fixture-backed content proves the assistant projections were consumed.
  await expect(page.locator('view-chat session-drawer')).toContainText('巡检');
  await expect(page.locator('view-chat global-hud')).toContainText('空闲');
  expect(consoleErrors).toEqual([]);
});
