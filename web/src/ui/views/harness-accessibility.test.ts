/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { render } from 'lit';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getHarnessOverview } from '../../api/harness.js';
import type { HarnessOverview } from '../../types/harness.js';
import {
  HarnessStatusCopy,
  formatHarnessTime,
  nextAnnouncement,
  renderHarnessStatus,
} from './harness-status.js';
import './harness-overview.js';
import type { HarnessOverviewView } from './harness-overview.js';

const sourceFile = (relativePath: string): string =>
  readFileSync(new URL(relativePath, import.meta.url), 'utf8');
const overviewCssSource = sourceFile('./harness-overview.css');
const overviewTsSource = sourceFile('./harness-overview.ts');
const statusTsSource = sourceFile('./harness-status.ts');
const sharedCssSource = sourceFile('../../styles/views/shared.css');

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    getHarnessOverview: vi.fn(),
    listHarnessSubagents: vi.fn(),
  };
});

const overviewApi = vi.mocked(getHarnessOverview);
let view: HarnessOverviewView | null = null;

function response<T>(result: T) {
  return { code: 0, msg: '', result };
}

function overview(): HarnessOverview {
  return {
    attention: { items: [], total: 0, limit: 5, offset: 0 },
    coordination: [
      {
        taskId: 'main:1',
        runId: 'run:1',
        objective: 'Main 巡检',
        status: 'unknown',
        waitReason: null,
        recentResult: null,
        updatedAt: '2026-11-03T10:00:00Z',
        subagents: [],
        subagentTotal: 0,
      },
    ],
  };
}

async function settle(host: HarnessOverviewView): Promise<void> {
  for (let index = 0; index < 5; index += 1) {
    await Promise.resolve();
    await host.updateComplete;
  }
  await new Promise((resolve) => setTimeout(resolve, 0));
  await host.updateComplete;
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/dashboard');
  overviewApi.mockReset();
});

afterEach(() => {
  view?.remove();
  view = null;
  localStorage.clear();
  vi.clearAllMocks();
});

describe('shared Harness status contract', () => {
  it('uses the exact Chinese status vocabulary', () => {
    expect(HarnessStatusCopy.scheduled?.label).toBe('计划');
    expect(HarnessStatusCopy.pending?.label).toBe('等待调度');
    expect(HarnessStatusCopy.unknown?.label).toBe('结果未知');
    expect(HarnessStatusCopy.cancelled_unknown?.label).toBe('停止结果未知');
    expect(HarnessStatusCopy.simulation?.label).toBe('模拟运行');
  });

  it('renders text plus an aria-hidden icon instead of relying on colour', () => {
    const host = document.createElement('div');
    render(renderHarnessStatus('unknown'), host);

    const status = host.querySelector<HTMLElement>('.harness-status')!;
    expect(status.textContent).toContain('结果未知');
    expect(status.querySelector('[aria-hidden="true"]')?.textContent?.trim()).not.toBe('');
    expect(status.getAttribute('data-status')).toBe('unknown');
    expect(status.className).toContain('harness-status--warn');
  });

  it('states the timezone beside every timestamp', () => {
    const formatted = formatHarnessTime('2026-11-03T10:05:00Z');
    expect(formatted).toContain('2026');
    expect(formatted).toMatch(/UTC|GMT/);
    expect(formatHarnessTime(null)).toBe('暂无');
  });

  it('dedupes repeated announcements', () => {
    const first = nextAnnouncement('', '已保存版本 v2');
    expect(first).toBe('已保存版本 v2');
    expect(nextAnnouncement(first, '已保存版本 v2')).toBe(first);
    expect(nextAnnouncement(first, '已停用计划')).toBe('已停用计划');
  });
});

describe('Harness overview accessibility', () => {
  it('keeps expanded state understandable on Main/Subagent rows', async () => {
    overviewApi.mockResolvedValue(response(overview()));
    const host = (view = document.createElement('view-harness-overview') as HarnessOverviewView);
    host.workspaceId = 'ws_1';
    document.body.append(host);
    await settle(host);

    const toggle = host.querySelector<HTMLButtonElement>('.harness-main__toggle')!;
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(toggle.getAttribute('aria-controls')).toBe('subagents-main:1');
    toggle.click();
    await settle(host);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
  });
});

describe('Harness responsive and design-system contracts', () => {
  it('uses the existing tokens and no new palette, fonts, or unsafe HTML', () => {
    const files = [overviewCssSource, overviewTsSource, statusTsSource];
    for (const source of files) {
      expect(source).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
      expect(source).not.toMatch(/rgb\(|rgba\(/);
      expect(source).not.toMatch(/unsafeHTML/);
      expect(source).not.toMatch(/\bfetch\(/);
    }
  });

  it('reflows at 980px and below without page-level horizontal scrolling', () => {
    const overviewCss = overviewCssSource;

    expect(overviewCss).toContain('@media (min-width: 980px)');
    expect(overviewCss).toContain('@media (max-width: 599px)');
    expect(overviewCss).toContain('min-height: 44px');
    expect(overviewCss).not.toContain('overflow-x: scroll');
    expect(overviewCss).not.toContain('overflow-x: auto');
    expect(overviewCss).toMatch(/\.harness-attention__action,[\s\S]*?min-height: 44px;/);
    expect(overviewCss).not.toContain('width: 100vw');
    expect(overviewCss).toContain('overflow-wrap');
  });

  it('keeps the shared status styles in a global entry, not only in a view stylesheet', () => {
    expect(sharedCssSource).toContain('.harness-status');
    expect(sharedCssSource).toContain('.harness-status__icon');
  });
});
