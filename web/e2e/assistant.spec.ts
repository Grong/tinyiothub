import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  // Authenticate before the app boots so direct hash navigation stays signed in.
  await page.addInitScript(() => {
    localStorage.setItem('auth-token', 'test-token');
    localStorage.setItem('workspace-id', 'ws-e2e');
  });
});

test('assistant workbench renders drawer, timeline, hud and composer', async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('pageerror', (error) => consoleErrors.push(error.message));

  // app.ts routes on pathname, so the chat route is `/chat`.
  await page.goto('/chat');
  await expect(page.locator('view-chat .assistant-topbar')).toBeVisible();
  await expect(page.locator('view-chat global-hud')).toBeVisible();
  await expect(page.locator('view-chat .composer-input')).toBeVisible();
  await expect(page.locator('view-chat .assistant-thread')).toBeVisible();
  await expect(page.locator('view-chat global-hud')).toContainText('待命');
  // The session list is a drawer: closed by default, opened from the top bar.
  await page.locator('view-chat .assistant-topbar .icon-button').click();
  await expect(page.locator('view-chat session-drawer')).toBeVisible();
  await expect(page.locator('view-chat session-drawer')).toContainText('巡检 · 当前');
  await expect(page.locator('view-chat session-drawer')).toContainText('运行中');
  await page.keyboard.press('Escape');
  await expect(page.locator('view-chat session-drawer')).toBeHidden();
  expect(consoleErrors).toEqual([]);
});

test('opens the assistant from a canonical #/assistant deep link', async ({ page }) => {
  await page.goto('/#/assistant');
  await expect(page.locator('view-chat .assistant-topbar')).toBeVisible();
  await expect(page.locator('view-chat global-hud')).toBeVisible();
  await expect(page.locator('view-chat .composer-input')).toBeVisible();
});

test('redirects legacy Runtime and Harness deep links to the assistant', async ({ page }) => {
  for (const legacy of [
    '#/runtime',
    '#/runtime/tasks/task%3A1?view=flow&device=dev%2F1',
    '#/harness',
    '#/harness/tasks/task%3A1?from=%23%2Fharness%23main-main%3A1',
  ]) {
    await page.goto(`/${legacy}`);
    await expect(page).toHaveURL(/#\/assistant$/);
    await expect(page.locator('view-chat global-hud')).toBeVisible();
  }
});
