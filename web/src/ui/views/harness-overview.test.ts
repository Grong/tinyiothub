import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import { getHarnessOverview, listHarnessSubagents } from '../../api/harness.js';
import type {
  HarnessAttentionItem,
  HarnessMainTaskSummary,
  HarnessOverview,
  HarnessSubagentSummary,
  HarnessWorkflowSummary,
} from '../../types/harness.js';
import './harness-overview.js';
import type { HarnessOverviewView } from './harness-overview.js';
import { parseHarnessHash, type HarnessRoute } from './harness-url-state.js';

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    getHarnessOverview: vi.fn(),
    listHarnessSubagents: vi.fn(),
    getHarnessWorkflow: vi.fn().mockResolvedValue({ code: 0, msg: '', result: null }),
    listHarnessWorkflowTemplates: vi
      .fn()
      .mockResolvedValue({ code: 0, msg: '', result: { templates: [] } }),
  };
});

const overviewApi = vi.mocked(getHarnessOverview);
const childrenApi = vi.mocked(listHarnessSubagents);
let view: HarnessOverviewView;

function attention(id: string): HarnessAttentionItem {
  return {
    id,
    kind: 'effect_unknown',
    title: `异常 ${id}`,
    detail: `详情 ${id}`,
    occurredAt: '2026-11-03T10:00:00Z',
    taskId: `task:${id}`,
    runId: `run:${id}`,
    eventId: `event:${id}`,
    actionLabel: '查看并处理',
  };
}

function workflow(id: string, enabled = true): HarnessWorkflowSummary {
  return {
    workflowId: id,
    objective: `工作流 ${id}`,
    enabled,
    version: 2,
    triggerLabel: enabled ? '每 5 分钟' : '已暂停',
    nextRunAt: enabled ? '2026-11-03T10:05:00Z' : null,
    lastResult: '上一次运行正常',
    updatedAt: '2026-11-03T09:55:00Z',
  };
}

function subagent(id: string): HarnessSubagentSummary {
  return {
    taskId: id,
    runId: `run:${id}`,
    status: 'active',
    waitReason: '等待父级汇总',
    recentResult: '子任务完成采样',
    updatedAt: '2026-11-03T10:01:00Z',
  };
}

function main(
  children: HarnessSubagentSummary[] = [],
  total = children.length,
): HarnessMainTaskSummary {
  return {
    taskId: 'main:1',
    runId: 'run:main',
    objective: '巡检一号泵',
    status: 'waiting_job',
    waitReason: '等待设备回执',
    recentResult: '已读取温度',
    updatedAt: '2026-11-03T10:02:00Z',
    subagents: children,
    subagentTotal: total,
  };
}

function overview(overrides: Partial<HarnessOverview> = {}): HarnessOverview {
  return {
    attention: { items: [attention('1')], total: 1, limit: 5, offset: 0 },
    workflows: [workflow('wf:1')],
    coordination: [main([subagent('child:1')], 1)],
    ...overrides,
  };
}

function response(result: HarnessOverview) {
  return { code: 0, msg: '', result };
}

async function settle(): Promise<void> {
  for (let index = 0; index < 4; index += 1) {
    await Promise.resolve();
    await view.updateComplete;
  }
}

async function mount(
  result: HarnessOverview = overview(),
  options: { anchor?: string; route?: HarnessRoute } = {},
): Promise<void> {
  overviewApi.mockResolvedValue(response(result));
  view = document.createElement('view-harness-overview') as HarnessOverviewView;
  view.workspaceId = 'ws_1';
  view.anchor = options.anchor ?? null;
  view.route = options.route ?? null;
  document.body.append(view);
  await settle();
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/');
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.clearAllMocks();
});

describe('Harness exception-first overview', () => {
  it('orders 需要处理 → Workflow 与调度 → 当前 Main/Subagent', async () => {
    await mount();

    const sections = [...view.querySelectorAll<HTMLElement>('[data-harness-section]')].map(
      (section) => section.dataset.harnessSection,
    );
    expect(sections).toEqual(['attention', 'workflows', 'coordination']);

    const headings = [...view.querySelectorAll('h2')].map((heading) => heading.textContent?.trim());
    expect(headings).toEqual(['需要处理', 'Workflow 与调度', '当前 Main / Subagent']);
    expect(view.textContent).toContain('结果未知');
  });

  it('renders cancelled_unknown as factual 停止结果未知 without claiming a stop', async () => {
    const cancelled = {
      ...main(),
      status: 'cancelled_unknown' as const,
      waitReason: null,
      recentResult: null,
    };
    await mount(overview({ coordination: [cancelled] }));

    expect(view.textContent).toContain('停止结果未知');
    expect(view.textContent).not.toContain('已停止');
  });

  it('has exactly one page-level primary action: 从模板新建', async () => {
    await mount();

    const primary = [...view.querySelectorAll<HTMLElement>('.btn--primary')];
    expect(primary).toHaveLength(1);
    expect(primary[0].textContent?.trim()).toBe('从模板新建');
    expect(primary[0].getAttribute('href')).toBe('#/harness/workflows/templates');
    expect(
      view.querySelector<HTMLAnchorElement>('.harness-overview__actions a[href="#/harness/workflows/new"]'),
    ).not.toBeNull();

    const h1 = view.querySelector('h1');
    expect(h1?.textContent).toContain('Harness');
  });

  it('routes the template-first journey to the templates view', async () => {
    await mount(overview(), { route: { name: 'templates' } });

    const templates = view.querySelector('view-harness-templates') as
      | (HTMLElement & { workspaceId: string })
      | null;
    expect(templates).not.toBeNull();
    expect(templates?.workspaceId).toBe('ws_1');
    expect(view.querySelector('.harness-overview')).toBeNull();
  });

  it('routes workflow creation and editing to the shared editor', async () => {
    await mount(overview(), {
      route: { name: 'workflows', workflowId: null, creating: true, version: null },
    });

    const editor = view.querySelector('view-harness-workflow-edit') as
      | (HTMLElement & { workflowId: string })
      | null;
    expect(editor).not.toBeNull();
    expect(editor?.workflowId).toBe('');
    expect(view.querySelector('.harness-overview')).toBeNull();

    view.remove();
    await mount(overview(), {
      route: { name: 'workflows', workflowId: 'wf:1', creating: false, version: null },
    });
    const existing = view.querySelector('view-harness-workflow-edit') as
      | (HTMLElement & { workflowId: string })
      | null;
    expect(existing?.workflowId).toBe('wf:1');
  });

  it('expands and scrolls the anchored Main when returning from a child Task', async () => {
    const original = (HTMLElement.prototype as unknown as { scrollIntoView?: unknown })
      .scrollIntoView;
    const scroll = vi.fn();
    (HTMLElement.prototype as unknown as { scrollIntoView?: unknown }).scrollIntoView = scroll;
    try {
      await mount(overview(), { anchor: 'main-main:1' });

      const toggle = view.querySelector<HTMLButtonElement>('.harness-main__toggle')!;
      expect(toggle.getAttribute('aria-expanded')).toBe('true');
      expect(view.querySelectorAll('.harness-subagent')).toHaveLength(1);
      expect(scroll).toHaveBeenCalled();
    } finally {
      (HTMLElement.prototype as unknown as { scrollIntoView?: unknown }).scrollIntoView = original;
    }
  });

  it('restores the attention page encoded in the return anchor', async () => {
    await mount(
      overview({
        attention: {
          items: ['6', '7'].map(attention),
          total: 7,
          limit: 5,
          offset: 5,
        },
      }),
      { anchor: 'harness-attention-5' },
    );

    expect(overviewApi.mock.calls[0][1]).toEqual({ attentionLimit: 5, attentionOffset: 5 });
    expect(view.textContent).toContain('第 6-7 项，共 7 项');
  });

  it('keeps aria-controls targets present while collapsed or empty', async () => {
    await mount(
      overview({
        attention: { items: [], total: 0, limit: 5, offset: 0 },
      }),
    );

    expect(view.querySelector('#harness-attention-list')).not.toBeNull();
    const mainToggle = view.querySelector<HTMLButtonElement>('.harness-main__toggle')!;
    expect(mainToggle.getAttribute('aria-controls')).toBe('subagents-main:1');
    expect(view.querySelector('#subagents-main\\:1')).not.toBeNull();
    expect(view.querySelector('#subagents-main\\:1')?.hasAttribute('hidden')).toBe(true);

    view.querySelector<HTMLButtonElement>('.harness-attention__toggle')!.click();
    await settle();
    expect(view.querySelector('#harness-attention-list')?.hasAttribute('hidden')).toBe(true);
  });

  it('uses the server-reported attention limit for paging', async () => {
    const first = overview({
      attention: {
        items: ['1', '2', '3'].map(attention),
        total: 7,
        limit: 3,
        offset: 0,
      },
    });
    await mount(first);

    overviewApi.mockResolvedValueOnce(
      response({
        ...first,
        attention: {
          items: ['4', '5', '6'].map(attention),
          total: 7,
          limit: 3,
          offset: 3,
        },
      }),
    );
    view.querySelector<HTMLButtonElement>('.harness-attention__next')!.click();
    await settle();

    expect(overviewApi.mock.calls[1][1]).toEqual({ attentionLimit: 5, attentionOffset: 3 });
  });

  it('keeps per-Main child errors across an aggregate attention refresh', async () => {
    const first = overview({
      attention: { items: ['1', '2', '3', '4', '5'].map(attention), total: 7, limit: 5, offset: 0 },
      coordination: [main([subagent('child:1')], 2)],
    });
    await mount(first);
    childrenApi.mockRejectedValueOnce(new ApiError(503, '子任务加载失败'));

    view.querySelector<HTMLButtonElement>('.harness-main__toggle')!.click();
    await settle();
    view.querySelector<HTMLButtonElement>('.harness-children__more')!.click();
    await settle();
    expect(view.querySelector('.harness-children__error')?.textContent).toContain('子任务加载失败');

    overviewApi.mockResolvedValueOnce(
      response({
        ...first,
        attention: { items: ['6', '7'].map(attention), total: 7, limit: 5, offset: 5 },
      }),
    );
    view.querySelector<HTMLButtonElement>('.harness-attention__next')!.click();
    await settle();

    expect(view.querySelector('.harness-children__error')?.textContent).toContain('子任务加载失败');
  });

  it('paginates and collapses attention items instead of stacking them', async () => {
    const first = overview({
      attention: {
        items: ['1', '2', '3', '4', '5'].map(attention),
        total: 7,
        limit: 5,
        offset: 0,
      },
    });
    await mount(first);
    expect(view.querySelectorAll('.harness-attention__item')).toHaveLength(5);
    expect(view.textContent).toContain('第 1-5 项，共 7 项');

    overviewApi.mockResolvedValueOnce(
      response({
        ...first,
        attention: {
          items: ['6', '7'].map(attention),
          total: 7,
          limit: 5,
          offset: 5,
        },
      }),
    );
    view.querySelector<HTMLButtonElement>('.harness-attention__next')!.click();
    await settle();

    expect(overviewApi.mock.calls[1][1]).toEqual({ attentionLimit: 5, attentionOffset: 5 });
    expect(view.querySelectorAll('.harness-attention__item')).toHaveLength(2);
    expect(view.textContent).toContain('第 6-7 项，共 7 项');
    expect(view.querySelector<HTMLButtonElement>('.harness-attention__previous')?.disabled).toBe(
      false,
    );

    const toggle = view.querySelector<HTMLButtonElement>('.harness-attention__toggle')!;
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    toggle.click();
    await settle();
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(toggle.textContent).toContain('展开');
    expect(view.querySelectorAll('.harness-attention__item')).toHaveLength(0);
    toggle.click();
    await settle();
    expect(view.querySelectorAll('.harness-attention__item')).toHaveLength(2);
  });

  it('shows workflows and scheduling without KPI cards', async () => {
    await mount();

    const workflows = view.querySelector('[data-harness-section="workflows"]')!;
    expect(workflows.textContent).toContain('工作流 wf:1');
    expect(workflows.textContent).toContain('已启用');
    expect(workflows.textContent).toContain('每 5 分钟');
    expect(workflows.textContent).toContain('下一次：');
    expect(view.querySelector('.harness-kpi')).toBeNull();
  });

  it('expands a Main into isolated Subagent rows with status, wait reason and recent result', async () => {
    await mount();
    expect(view.querySelectorAll('.harness-subagent')).toHaveLength(0);

    const toggle = view.querySelector<HTMLButtonElement>('.harness-main__toggle')!;
    expect(toggle.tagName).toBe('BUTTON');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(toggle.getAttribute('tabindex')).not.toBe('-1');
    expect(toggle.getAttribute('aria-controls')).toBe('subagents-main:1');

    toggle.click();
    await settle();

    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    const rows = view.querySelectorAll('.harness-subagent');
    expect(rows).toHaveLength(1);
    expect(rows[0].textContent).toContain('进行中');
    expect(rows[0].textContent).toContain('等待父级汇总');
    expect(rows[0].textContent).toContain('子任务完成采样');

    const childLink = rows[0].querySelector<HTMLAnchorElement>('a')!;
    const route = parseHarnessHash(childLink.getAttribute('href')!);
    expect(route).toMatchObject({
      name: 'task',
      taskId: 'child:1',
      from: '#/harness#main-main:1',
    });

    // No DAG rendering: the coordination area is a two-level list only.
    expect(view.querySelector('[data-harness-section="coordination"] svg')).toBeNull();
  });

  it('loads more Subagents incrementally and reports the parent return path', async () => {
    await mount(overview({ coordination: [main([subagent('child:1')], 2)] }));
    childrenApi.mockResolvedValue({
      code: 0,
      msg: '',
      result: {
        subagents: [subagent('child:2')],
        total: 2,
        limit: 20,
        offset: 1,
      },
    });

    view.querySelector<HTMLButtonElement>('.harness-main__toggle')!.click();
    await settle();
    view.querySelector<HTMLButtonElement>('.harness-children__more')!.click();
    await settle();

    expect(childrenApi).toHaveBeenCalledOnce();
    expect(childrenApi.mock.calls[0].slice(0, 4)).toEqual(['ws_1', 'main:1', 20, 1]);
    expect(view.querySelectorAll('.harness-subagent')).toHaveLength(2);
    expect(view.querySelector('.harness-children__more')).toBeNull();
    expect(view.textContent).toContain('已显示 2 / 共 2');
    expect(view.querySelector('.harness-main')?.getAttribute('id')).toBe('main-main:1');
  });

  it('does not render misleading empty sections before the first successful load', async () => {
    overviewApi.mockRejectedValue(new ApiError(404, '接口未提供'));
    view = document.createElement('view-harness-overview') as HarnessOverviewView;
    view.workspaceId = 'ws_1';
    document.body.append(view);
    await settle();

    expect(view.querySelector('[role="alert"]')?.textContent).toContain('接口未提供');
    expect(view.textContent).toContain('无法加载，请重试');
    expect(view.textContent).not.toContain('显示上次结果');
    expect(view.textContent).not.toContain('尚未配置 Workflow');
    expect(view.querySelectorAll('[data-harness-section]')).toHaveLength(0);
  });

  it('shows Chinese empty states, the template-first action and a visible error state', async () => {
    await mount(
      overview({
        attention: { items: [], total: 0, limit: 5, offset: 0 },
        workflows: [],
        coordination: [],
      }),
    );
    expect(view.textContent).toContain('当前没有需要处理的事项');
    expect(view.textContent).toContain('尚未配置 Workflow');
    expect(view.textContent).toContain('当前没有 Main 任务');
    const primaries = [...view.querySelectorAll<HTMLElement>('.btn--primary')];
    expect(primaries).toHaveLength(1);
    expect(primaries[0].textContent?.trim()).toBe('从模板新建');
    const emptyActions = view.querySelector<HTMLElement>('.harness-empty__actions');
    expect(emptyActions?.querySelector('.btn--primary')?.textContent?.trim()).toBe('从模板新建');
    expect(
      emptyActions?.querySelector<HTMLAnchorElement>('a[href="#/harness/workflows/templates"]'),
    ).not.toBeNull();
    expect(
      emptyActions?.querySelector<HTMLAnchorElement>('a[href="#/harness/workflows/new"]'),
    ).not.toBeNull();

    overviewApi.mockRejectedValueOnce(new ApiError(503, '服务暂不可用'));
    view.querySelector<HTMLButtonElement>('.harness-overview__retry')!.click();
    await settle();
    expect(view.querySelector('[role="alert"]')?.textContent).toContain('服务暂不可用');
  });
});
