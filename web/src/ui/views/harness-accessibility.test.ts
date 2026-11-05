/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { render } from 'lit';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getHarnessOverview, getHarnessWorkflow } from '../../api/harness.js';
import type { HarnessOverview, WorkflowRevision } from '../../types/harness.js';
import {
  HarnessStatusCopy,
  formatHarnessTime,
  nextAnnouncement,
  renderHarnessStatus,
} from './harness-status.js';
import './harness-overview.js';
import type { HarnessOverviewView } from './harness-overview.js';
import './harness-workflow-edit.js';
import type { HarnessWorkflowEditView } from './harness-workflow-edit.js';

const sourceFile = (relativePath: string): string =>
  readFileSync(new URL(relativePath, import.meta.url), 'utf8');
const editorCssSource = sourceFile('./harness-workflow-edit.css');
const overviewCssSource = sourceFile('./harness-overview.css');
const editorTsSource = sourceFile('./harness-workflow-edit.ts');
const overviewTsSource = sourceFile('./harness-overview.ts');
const statusTsSource = sourceFile('./harness-status.ts');
const sharedCssSource = sourceFile('../../styles/views/shared.css');

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    getHarnessOverview: vi.fn(),
    listHarnessSubagents: vi.fn(),
    getHarnessWorkflow: vi.fn(),
  };
});

const overviewApi = vi.mocked(getHarnessOverview);
const workflowApi = vi.mocked(getHarnessWorkflow);
let view: HarnessOverviewView | HarnessWorkflowEditView | null = null;

function response<T>(result: T) {
  return { code: 0, msg: '', result };
}

function revision(): WorkflowRevision {
  return {
    workflowId: 'wf:1',
    version: 2,
    enabled: true,
    definition: {
      objective: '巡检',
      trigger: { type: 'cron', triggerId: 't1', expression: '0 * * * *' },
      deviceSelector: {},
      sourceSelector: {},
      toolIds: [],
      skillRefs: [],
      requiredCapabilities: [],
      approvalPolicy: {},
      evidencePolicy: {},
    },
    createdAt: '2026-11-03T10:00:00Z',
  };
}

function overview(): HarnessOverview {
  return {
    attention: { items: [], total: 0, limit: 5, offset: 0 },
    workflows: [
      {
        workflowId: 'wf:1',
        objective: '巡检一号泵',
        enabled: true,
        version: 2,
        triggerLabel: '每 5 分钟',
        nextRunAt: '2026-11-03T10:05:00Z',
        lastResult: null,
        updatedAt: '2026-11-03T09:55:00Z',
      },
    ],
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

async function settle(host: HarnessOverviewView | HarnessWorkflowEditView): Promise<void> {
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
  workflowApi.mockReset();
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
  it('renders workflow rows as a semantic table with header scopes', async () => {
    overviewApi.mockResolvedValue(response(overview()));
    const host = (view = document.createElement('view-harness-overview') as HarnessOverviewView);
    host.workspaceId = 'ws_1';
    document.body.append(host);
    await settle(host);

    const headers = [...host.querySelectorAll<HTMLElement>('.harness-workflow-table thead th')];
    expect(headers.length).toBeGreaterThan(0);
    expect(headers.every((header) => header.getAttribute('scope') === 'col')).toBe(true);
    expect(
      host.querySelector('.harness-workflow-table tbody th')?.getAttribute('scope'),
    ).toBe('row');
    expect(host.querySelector('.harness-workflow-table thead')?.getAttribute('role')).toBe(
      'rowgroup',
    );
    expect(host.querySelector('.harness-workflow-table tbody')?.getAttribute('role')).toBe(
      'rowgroup',
    );
    expect(host.querySelectorAll('.harness-workflow-table__field')).toHaveLength(6);
    // Explicit roles keep the table contract even when narrow CSS reflows rows.
    expect(host.querySelector('.harness-workflow-table')?.getAttribute('role')).toBe('table');
    expect(host.querySelector('.harness-workflow-table tbody tr')?.getAttribute('role')).toBe(
      'row',
    );
    expect(host.querySelector('.harness-workflow-table tbody td')?.getAttribute('role')).toBe(
      'cell',
    );
    expect(host.querySelector('.harness-workflow-table tbody th')?.getAttribute('role')).toBe(
      'rowheader',
    );

    const status = host.querySelector<HTMLElement>('.harness-workflow .harness-status')!;
    expect(status.textContent).toContain('已启用');
    expect(status.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });

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

describe('Harness workflow editor accessibility', () => {
  it('keeps exactly one persistent polite announcer alongside transient status text', async () => {
    workflowApi.mockResolvedValue(response(revision()));
    const host = (view = document.createElement(
      'view-harness-workflow-edit',
    ) as HarnessWorkflowEditView);
    host.workspaceId = 'ws_1';
    host.workflowId = 'wf:1';
    document.body.append(host);
    await settle(host);

    expect(host.querySelectorAll('[data-announcer][role="status"]')).toHaveLength(1);
    expect(host.querySelector('[data-announcer]')?.getAttribute('aria-live')).toBe('polite');
  });

  it('returns focus to the trigger after a dialog closes', async () => {
    workflowApi.mockResolvedValue(response(revision()));
    const host = (view = document.createElement(
      'view-harness-workflow-edit',
    ) as HarnessWorkflowEditView);
    host.workspaceId = 'ws_1';
    host.workflowId = 'wf:1';
    document.body.append(host);
    await settle(host);

    const trigger = host.querySelector<HTMLButtonElement>('button[data-action="toggle-enabled"]')!;
    trigger.focus();
    trigger.click();
    await settle(host);
    expect(host.querySelector('[role="dialog"]')).not.toBeNull();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle(host);
    expect(host.querySelector('[role="dialog"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
});

describe('Harness responsive and design-system contracts', () => {
  it('uses the existing tokens and no new palette, fonts, or unsafe HTML', () => {
    const files = [
      editorCssSource,
      overviewCssSource,
      editorTsSource,
      overviewTsSource,
      statusTsSource,
    ];
    for (const source of files) {
      expect(source).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
      expect(source).not.toMatch(/rgb\(|rgba\(/);
      expect(source).not.toMatch(/unsafeHTML/);
      expect(source).not.toMatch(/\bfetch\(/);
    }
    expect(editorCssSource).toContain('var(--space-');
    expect(editorCssSource).toContain('var(--focus-ring)');
    expect(editorCssSource).toContain('var(--danger-subtle)');
  });

  it('reflows at 980px, 600px and below without page-level horizontal scrolling', () => {
    const editorCss = editorCssSource;
    const overviewCss = overviewCssSource;

    expect(editorCss).toContain('@media (min-width: 980px)');
    expect(editorCss).toContain('@media (max-width: 599px)');
    expect(editorCss).toContain('min-height: 44px');
    expect(overviewCss).toContain('@media (min-width: 980px)');
    expect(overviewCss).toContain('@media (max-width: 599px)');
    expect(overviewCss).toContain('.harness-workflow-table');
    expect(overviewCss).toContain('.harness-workflow-table__field');
    expect(overviewCss).not.toContain('overflow-x: scroll');
    expect(overviewCss).not.toContain('overflow-x: auto');
    expect(overviewCss).toMatch(
      /\.harness-attention__action,[\s\S]*?\.harness-workflow__action[\s\S]*?min-height: 44px;/,
    );

    for (const source of [editorCss, overviewCss]) {
      expect(source).not.toContain('width: 100vw');
      expect(source).toContain('overflow-wrap');
    }
  });

  it('keeps the shared status styles in a global entry, not only in a view stylesheet', () => {
    expect(sharedCssSource).toContain('.harness-status');
    expect(sharedCssSource).toContain('.harness-status__icon');
  });
});
