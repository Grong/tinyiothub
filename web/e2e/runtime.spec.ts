import AxeBuilder from '@axe-core/playwright';
import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

const FIXTURE = 'http://127.0.0.1:3002';

interface FixtureCounts {
  mediaRequests: number;
  cancelRequests: number;
  decisionRequests: Record<string, number>;
}

async function counts(request: APIRequestContext): Promise<FixtureCounts> {
  const response = await request.get(`${FIXTURE}/__fixtures__/counts`);
  return response.json() as Promise<FixtureCounts>;
}

async function mediaRequests(request: APIRequestContext): Promise<number> {
  return (await counts(request)).mediaRequests;
}

async function decisionRequests(request: APIRequestContext, taskId: string): Promise<number> {
  return (await counts(request)).decisionRequests[taskId] ?? 0;
}

async function releaseDelayedDecision(request: APIRequestContext): Promise<void> {
  await request.post(`${FIXTURE}/__fixtures__/release-decision`);
}

async function openRuntimeTask(
  page: Page,
  taskId: string,
  history = taskId.endsWith('_history'),
): Promise<void> {
  await page.goto(`/#/runtime/tasks/${encodeURIComponent(taskId)}`);
  await expect(page.locator('.runtime-task[aria-busy="false"]')).toBeVisible();
  if (!history) return;
  await page.getByRole('tab', { name: 'Flow' }).click();
  await expect(page.getByRole('button', { name: /Historical event/ }).first()).toBeVisible();
  await page
    .getByRole('button', { name: /Historical event/ })
    .first()
    .click();
  await expect(page.getByText('History · read-only')).toBeVisible();
}

async function activePlayers(page: Page): Promise<number> {
  return page.evaluate(
    () => (window as Window & { __runtimeActivePlayers?: number }).__runtimeActivePlayers ?? 0,
  );
}

test.beforeEach(async ({ page, request }) => {
  await request.post(`${FIXTURE}/__fixtures__/reset`);
  await page.addInitScript(() => {
    localStorage.setItem('auth-token', 'e2e-token');
    localStorage.setItem('workspace-id', 'ws_e2e');
    localStorage.setItem(
      'openclaw.control.settings.v1',
      JSON.stringify({ theme: 'dark', chatShowThinking: true, sessionKey: 'main' }),
    );
  });
});

test('task switch destroys playback and blocks stale media', async ({ page, request }) => {
  await openRuntimeTask(page, 'task_video');
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.locator('video')).toBeVisible();
  await expect.poll(() => activePlayers(page)).toBe(1);

  await page.getByRole('button', { name: 'Switch task' }).click();
  await page.getByRole('option', { name: 'task_without_video' }).click();
  await expect(page.locator('video')).toHaveCount(0);
  await expect(page).toHaveURL(/#\/runtime\/tasks\/task_without_video/);

  const oldOriginRequests = await mediaRequests(request);
  await page.waitForTimeout(1500);
  expect(await mediaRequests(request)).toBe(oldOriginRequests);
  expect(await activePlayers(page)).toBe(0);
});

test('entering history destroys live playback on the same Task', async ({ page }) => {
  await openRuntimeTask(page, 'task_video', false);
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.locator('video')).toBeVisible();
  await expect.poll(() => activePlayers(page)).toBe(1);

  await page.getByRole('tab', { name: 'Flow' }).click();
  await page
    .getByRole('button', { name: /Historical event/ })
    .first()
    .click();
  await expect(page.getByText('History · read-only')).toBeVisible();
  await expect(page.locator('video')).toHaveCount(0);
  await expect.poll(() => activePlayers(page)).toBe(0);
});

test('expired image evidence is shown as unavailable without media or a player', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_expired_evidence');
  const before = await mediaRequests(request);
  await expect(page.getByText('Image evidence unavailable (expired)')).toBeVisible();
  await expect(page.locator('button[data-evidence-id="evidence_expired"]')).toHaveCount(0);
  await expect(page.locator('video')).toHaveCount(0);
  expect(await mediaRequests(request)).toBe(before);
  expect(await activePlayers(page)).toBe(0);
});

test('initial timeline seeds Loop and Flow before SSE opens at the newest cursor', async ({
  page,
}) => {
  const streamUrls: string[] = [];
  page.on('request', (request) => {
    if (request.url().includes('/agent/tasks/task_timeline/stream')) streamUrls.push(request.url());
  });
  await openRuntimeTask(page, 'task_timeline');

  const loop = page.locator('runtime-loop');
  await expect(loop.locator('[data-event-id="event_1"]')).toHaveCount(1);
  await expect(loop.locator('[data-event-id="event_3"]')).toHaveCount(1);
  await page.getByRole('tab', { name: 'Flow' }).click();
  const flow = page.locator('runtime-flow');
  await expect(flow.locator('[data-event-id="event_1"]')).toHaveCount(1);
  await expect(flow.locator('[data-event-id="event_3"]')).toHaveCount(1);
  await expect.poll(() => streamUrls.some((url) => /[?&]after=3(?:&|$)/.test(url))).toBe(true);
});

test('load earlier pages backwards from the oldest seeded cursor', async ({ page }) => {
  await openRuntimeTask(page, 'task_timeline_paged');
  const loop = page.locator('runtime-loop');
  await expect(loop.locator('[data-event-id="event_21"]')).toHaveCount(1);
  await expect(loop.locator('[data-event-id="event_120"]')).toHaveCount(1);
  await expect(loop.locator('[data-event-id="event_1"]')).toHaveCount(0);

  await page.getByRole('button', { name: 'Load earlier events' }).click();

  await expect(loop.locator('[data-event-id="event_1"]')).toHaveCount(1);
  await expect(loop.locator('[data-event-id="event_21"]')).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Load earlier events' })).toHaveCount(0);
});

test('device change destroys the old playback before playing the new device', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_two_cameras');
  await page.getByRole('button', { name: 'Play evidence' }).click();
  const image = page.locator('runtime-playback img');
  await expect(image).toHaveAttribute('src', /frame-a\.png/);
  await expect(image).toHaveAttribute('alt', /cam-a/);
  const afterA = await mediaRequests(request);
  expect(afterA).toBeGreaterThan(0);

  await page.locator('runtime-loop [data-device-id="cam-b"]').click();
  await expect(page.locator('runtime-playback')).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await mediaRequests(request)).toBe(afterA);

  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.locator('runtime-playback img')).toHaveAttribute('src', /frame-b\.png/);
  await expect(page.locator('runtime-playback img')).toHaveAttribute('alt', /cam-b/);
  await expect(page.locator('runtime-playback img')).not.toHaveAttribute('src', /frame-a\.png/);
});

test('expired MJPEG and unsafe playback URLs fail closed without media requests', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_expired_mjpeg');
  const beforeExpired = await mediaRequests(request);
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.getByText('playback_unavailable', { exact: true })).toBeVisible();
  await expect(page.locator('runtime-playback')).toHaveCount(0);
  expect(await mediaRequests(request)).toBe(beforeExpired);

  await openRuntimeTask(page, 'task_unsafe_playback');
  const beforeUnsafe = await mediaRequests(request);
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.getByText('playback_unavailable', { exact: true })).toBeVisible();
  await expect(page.locator('runtime-playback')).toHaveCount(0);
  expect(await mediaRequests(request)).toBe(beforeUnsafe);
});

test('approval decisions meet 44px touch targets at 360px', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await openRuntimeTask(page, 'task_approval');
  for (const name of ['Approve once', 'Reject']) {
    const button = page.getByRole('button', { name, exact: true });
    await expect(button).toBeVisible();
    const style = await button.evaluate((element) => {
      const computed = getComputedStyle(element);
      const box = element.getBoundingClientRect();
      return {
        minWidth: computed.minWidth,
        minHeight: computed.minHeight,
        width: box.width,
        height: box.height,
      };
    });
    expect(style.minWidth, name).toBe('44px');
    expect(style.minHeight, name).toBe('44px');
    expect(style.width, name).toBeGreaterThanOrEqual(44);
    expect(style.height, name).toBeGreaterThanOrEqual(44);
  }
});

test('switch-task dialog traps Tab and closes on Escape from outside', async ({ page }) => {
  await openRuntimeTask(page, 'task_timeline');
  const opener = page.getByRole('button', { name: 'Switch task' });
  await opener.click();
  const dialog = page.getByRole('dialog', { name: 'Switch task' });
  await expect(dialog).toBeVisible();
  const first = dialog.getByRole('option').first();
  const last = dialog.getByRole('button', { name: 'Keep current task' });
  await expect(first).toBeFocused();

  await page.keyboard.press('Shift+Tab');
  await expect(last).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(first).toBeFocused();

  // Escape must work even when focus escaped the dialog.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(opener).toBeFocused();
});

test('browser back and forward restore the active Runtime view', async ({ page }) => {
  await openRuntimeTask(page, 'task_timeline');
  await page.getByRole('tab', { name: 'Flow' }).click();
  await expect(page).toHaveURL(/view=flow/);
  await page.getByRole('tab', { name: 'Agent Loop' }).click();
  await expect(page).not.toHaveURL(/view=flow/);

  await page.goBack();
  await expect(page).toHaveURL(/view=flow/);
  await expect(page.getByRole('tab', { name: 'Flow' })).toHaveAttribute('aria-selected', 'true');
  await page.goForward();
  await expect(page.getByRole('tab', { name: 'Agent Loop' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
});

test('persisted collapsed rail expands at tablet width', async ({ page }) => {
  await page.setViewportSize({ width: 768, height: 900 });
  await page.addInitScript(() => localStorage.setItem('runtime.devices.expanded', 'false'));
  await openRuntimeTask(page, 'task_timeline');
  const rail = page.getByRole('button', { name: 'Expand devices' });
  await expect(rail).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Associated devices' })).not.toBeVisible();

  await rail.click();
  await expect(page.getByRole('heading', { name: 'Associated devices' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Expand devices' })).toHaveCount(0);
});

test('playback at 360px does not overflow horizontally', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await openRuntimeTask(page, 'task_two_cameras');
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.locator('runtime-playback img')).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});

test('CORS success plays direct fixture media and CORS failure is visible', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_cors_allowed');
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.locator('video')).toBeVisible();
  await expect.poll(() => mediaRequests(request)).toBeGreaterThan(0);
  await expect(page.getByText('Playback unavailable')).toHaveCount(0);

  await openRuntimeTask(page, 'task_cors_denied');
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.getByText('playback_unavailable', { exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await expect.poll(() => mediaRequests(request)).toBeGreaterThan(0);
  await expect.poll(() => activePlayers(page)).toBe(0);
});

test('expired signatures and unsupported protocols fail closed without media requests', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_expired_signature');
  const beforeExpired = await mediaRequests(request);
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.getByText('playback_unavailable', { exact: true })).toBeVisible();
  expect(await mediaRequests(request)).toBe(beforeExpired);
  expect(await activePlayers(page)).toBe(0);

  await openRuntimeTask(page, 'task_unsupported_protocol');
  const beforeUnsupported = await mediaRequests(request);
  await page.getByRole('button', { name: 'Play evidence' }).click();
  await expect(page.getByText('playback_unavailable', { exact: true })).toBeVisible();
  expect(await mediaRequests(request)).toBe(beforeUnsupported);
  await expect(page.locator('video')).toHaveCount(0);
});

test('responsive tabs, keyboard history, reduced motion, and AA remain usable', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openRuntimeTask(page, 'task_history');
  await page.getByRole('tab', { name: 'Devices' }).click();
  await expect(page.getByRole('heading', { name: 'Associated devices' })).toBeVisible();
  await page.getByRole('tab', { name: 'Flow' }).click();
  await page.getByRole('button', { name: 'Historical event event_2' }).click();
  await expect(page.getByText('History · read-only')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Cancel task' })).toHaveCount(0);
  expect(await page.locator('.runtime-data-particles').isVisible()).toBe(false);

  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa']).analyze();
  expect(results.violations).toEqual([]);
});

test('approval stays single-flight and stale decisions cannot cross Tasks', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_approval');
  const approve = page.getByRole('button', { name: 'Approve once' });
  await approve.evaluate((button: HTMLButtonElement) => {
    button.click();
    button.click();
  });
  await expect(page.getByText('Approved', { exact: true })).toBeVisible();
  expect(await decisionRequests(request, 'task_approval')).toBe(1);

  await openRuntimeTask(page, 'task_approval_delayed');
  await page.getByRole('button', { name: 'Approve once' }).click();
  await page.getByRole('button', { name: 'Switch task' }).click();
  await page.getByRole('option', { name: 'task_without_video' }).click();
  expect(await decisionRequests(request, 'task_approval_delayed')).toBe(1);
  await releaseDelayedDecision(request);
  await expect(page.getByRole('button', { name: 'Approve once' })).toHaveCount(0);
  await expect(page).toHaveURL(/#\/runtime\/tasks\/task_without_video/);

  await openRuntimeTask(page, 'task_approval_history');
  await expect(page.getByText('History · read-only')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve once' })).toHaveCount(0);
});

test('approval reject, conflict, expiry and unavailable expose current facts only', async ({
  page,
}) => {
  await openRuntimeTask(page, 'task_reject');
  await page.getByRole('button', { name: 'Reject' }).click();
  await expect(page.getByText('Rejected', { exact: true })).toBeVisible();

  await openRuntimeTask(page, 'task_approval_conflict');
  await page.getByRole('button', { name: 'Approve once' }).click();
  await expect(page.getByRole('alert')).toContainText('conflict');
  await expect(page.getByText('Approved', { exact: true })).toHaveCount(0);

  await openRuntimeTask(page, 'task_approval_expired');
  await expect(page.getByText('Expired · this approval request is no longer valid.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve once' })).toHaveCount(0);

  await openRuntimeTask(page, 'task_approval_unavailable');
  await expect(
    page.getByText('Unavailable · this approval request cannot be decided.'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve once' })).toHaveCount(0);
});

test('stale SSE duplicates are suppressed and resync keeps one event', async ({ page }) => {
  await openRuntimeTask(page, 'task_sse_resync');
  await expect(page.locator('[data-event-id="event_1"]')).toHaveCount(1);
  // The fixture replays a newer event twice and then resets the cursor; the
  // live projection must keep exactly one copy of each.
  await expect(page.locator('[data-event-id="event_2"]')).toHaveCount(1);
  await page.waitForTimeout(500);
  await expect(page.locator('[data-event-id="event_1"]')).toHaveCount(1);
  await expect(page.locator('[data-event-id="event_2"]')).toHaveCount(1);
});

test('seam supports keyboard and pointer resize at desktop width', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await openRuntimeTask(page, 'task_video');
  const seam = page.locator('[role="separator"]');
  await seam.focus();
  const initial = Number(await seam.getAttribute('aria-valuenow'));
  await seam.press('ArrowLeft');
  await expect(seam).toHaveAttribute('aria-valuenow', String(initial + 24));

  const beforeDrag = Number(await seam.getAttribute('aria-valuenow'));
  const box = await seam.boundingBox();
  if (!box) throw new Error('seam box missing');
  // Avoid the centered collapse toggle so the drag targets the handle itself.
  const dragY = box.y + Math.min(24, box.height / 4);
  await page.mouse.move(box.x + box.width / 2, dragY);
  await page.mouse.down();
  await page.mouse.move(box.x - 80, dragY, { steps: 5 });
  await page.mouse.up();
  await expect
    .poll(async () => Number(await seam.getAttribute('aria-valuenow')))
    .toBeGreaterThan(beforeDrag);
});

test('destructive confirmation traps focus, defaults safe, and returns focus', async ({
  page,
  request,
}) => {
  await openRuntimeTask(page, 'task_cancel');
  const overflow = page.getByRole('button', { name: 'More task actions' });
  await overflow.click();
  await page.getByRole('menuitem', { name: 'Cancel task' }).click();
  const dialog = page.getByRole('dialog', { name: 'Cancel task' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Keep running' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(dialog.getByRole('button', { name: 'Cancel task' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(dialog.getByRole('button', { name: 'Keep running' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(overflow).toBeFocused();
  expect((await counts(request)).cancelRequests).toBe(0);

  await overflow.click();
  await page.getByRole('menuitem', { name: 'Cancel task' }).click();
  await page
    .getByRole('dialog', { name: 'Cancel task' })
    .getByRole('button', { name: 'Cancel task' })
    .click();
  await expect.poll(async () => (await counts(request)).cancelRequests).toBe(1);
});

test('360px has no horizontal overflow and 200% zoom keeps controls reachable', async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 800 });
  await openRuntimeTask(page, 'task_video');
  const overflow360 = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow360).toBeLessThanOrEqual(1);
  await expect(page.getByRole('tab', { name: 'Agent Loop' })).toBeVisible();

  await page.setViewportSize({ width: 640, height: 800 });
  const overflowZoom = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflowZoom).toBeLessThanOrEqual(1);
  await expect(page.getByRole('button', { name: 'Play evidence' })).toBeVisible();
});

test('768px keeps tabs and the devices rail without horizontal overflow', async ({ page }) => {
  await page.setViewportSize({ width: 768, height: 900 });
  await openRuntimeTask(page, 'task_history');
  await expect(page.getByRole('tab', { name: 'Agent Loop' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Flow' })).toBeVisible();
  await expect(page.getByRole('tab', { name: 'Devices' })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Associated devices' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Devices', exact: true })).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
});

test('light and dark Runtime surfaces meet WCAG AA', async ({ page }) => {
  await openRuntimeTask(page, 'task_video');
  for (const theme of ['light', 'dark']) {
    await page.evaluate((next) => {
      if (next === 'light') document.documentElement.setAttribute('data-theme', 'light');
      else document.documentElement.removeAttribute('data-theme');
    }, theme);
    const results = await new AxeBuilder({ page })
      .include('.runtime-task')
      .withTags(['wcag2a', 'wcag2aa'])
      .analyze();
    expect(results.violations, `${theme} violations`).toEqual([]);
  }
});

test('empty, error, offline and partial projections stay honest', async ({ page }) => {
  await openRuntimeTask(page, 'task_empty');
  await expect(page.getByText('No agent activity yet')).toBeVisible();
  await expect(page.getByText('No referenced devices')).toBeVisible();

  await page.goto('/#/runtime/tasks/task_error');
  await expect(page.getByRole('alert')).toContainText('fixture failure');

  await openRuntimeTask(page, 'task_offline');
  await expect(page.locator('[data-connection="offline"]')).toBeVisible({ timeout: 15_000 });

  await openRuntimeTask(page, 'task_partial');
  await expect(page.locator('.runtime-task')).toBeVisible();
  await expect(page.locator('[data-device-card][data-device-id="ghost-device"]')).toBeVisible();
  await expect(page.locator('[data-device-card][data-device-id="cam-allowed"]')).toBeVisible();
});
