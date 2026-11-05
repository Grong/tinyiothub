import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, type ApiResponse } from '../../api/client.js';
import {
  createHarnessWorkflow,
  getHarnessWorkflow,
  setHarnessWorkflowEnabled,
  updateHarnessWorkflow,
} from '../../api/harness.js';
import type { WorkflowDefinition, WorkflowRevision } from '../../types/harness.js';
import './harness-workflow-edit.js';
import {
  WORKFLOW_CONFLICT_MESSAGE,
  workflowFieldErrors,
  type HarnessWorkflowEditView,
} from './harness-workflow-edit.js';

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    getHarnessWorkflow: vi.fn(),
    createHarnessWorkflow: vi.fn(),
    updateHarnessWorkflow: vi.fn(),
    setHarnessWorkflowEnabled: vi.fn(),
  };
});

const getWorkflow = vi.mocked(getHarnessWorkflow);
const createWorkflow = vi.mocked(createHarnessWorkflow);
const updateWorkflow = vi.mocked(updateHarnessWorkflow);
const setEnabled = vi.mocked(setHarnessWorkflowEnabled);
let view: HarnessWorkflowEditView;

function definition(overrides: Partial<WorkflowDefinition> = {}): WorkflowDefinition {
  return {
    objective: '巡检一号泵',
    trigger: { type: 'cron', triggerId: 't1', expression: '0 * * * *' },
    deviceSelector: { deviceIds: ['pump-1'] },
    sourceSelector: {},
    toolIds: ['read_device'],
    skillRefs: ['inspection'],
    requiredCapabilities: ['text'],
    approvalPolicy: {},
    evidencePolicy: {},
    ...overrides,
  };
}

function revision(overrides: Partial<WorkflowRevision> = {}): WorkflowRevision {
  return {
    workflowId: 'wf:1',
    version: 3,
    enabled: true,
    definition: definition(),
    createdAt: '2026-11-03T10:00:00Z',
    ...overrides,
  };
}

function response<T>(result: T) {
  return { code: 0, msg: '', result };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function settle(): Promise<void> {
  for (let index = 0; index < 5; index += 1) {
    await Promise.resolve();
    await view.updateComplete;
  }
  await new Promise((resolve) => setTimeout(resolve, 0));
  await view.updateComplete;
}

async function mount(
  options: { workflowId?: string; loaded?: WorkflowRevision | Error } = {},
): Promise<void> {
  view = document.createElement('view-harness-workflow-edit') as HarnessWorkflowEditView;
  view.workspaceId = 'ws_1';
  view.workflowId = options.workflowId ?? '';
  if (options.loaded instanceof Error) {
    getWorkflow.mockRejectedValueOnce(options.loaded);
  } else if (options.loaded) {
    getWorkflow.mockResolvedValueOnce(response(options.loaded));
  }
  document.body.append(view);
  await settle();
}

function field(id: string): HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement {
  const element = view.querySelector<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(
    `#${id}`,
  );
  if (!element) throw new Error(`Missing field ${id}`);
  return element;
}

function type(id: string, value: string): void {
  const element = field(id);
  element.value = value;
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

function click(selector: string): void {
  const element = view.querySelector<HTMLElement>(selector);
  if (!element) throw new Error(`Missing control ${selector}`);
  element.click();
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/dashboard');
  getWorkflow.mockReset();
  createWorkflow.mockReset();
  updateWorkflow.mockReset();
  setEnabled.mockReset();
});

afterEach(() => {
  view?.remove();
  document.querySelectorAll('.harness-test-outside').forEach((element) => element.remove());
  localStorage.clear();
  vi.clearAllMocks();
});

describe('Harness workflow editor form', () => {
  it('renders the five groups in order with persistent labels and anchors', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });

    const groups = [...view.querySelectorAll<HTMLElement>('[data-group]')].map(
      (group) => group.dataset.group,
    );
    expect(groups).toEqual(['objective', 'trigger', 'tools', 'policy', 'summary']);
    expect([...view.querySelectorAll('h2')].map((heading) => heading.textContent?.trim())).toEqual([
      '目标与设备',
      '触发',
      '工具与 Skills',
      '策略与证据',
      '配置摘要',
    ]);
    expect(
      [...view.querySelectorAll<HTMLElement>('[data-group-link]')].map(
        (link) => link.dataset.groupLink,
      ),
    ).toEqual(['objective', 'trigger', 'tools', 'policy', 'summary']);

    for (const element of view.querySelectorAll<HTMLElement>('input, textarea, select')) {
      expect(element.id).not.toBe('');
      expect(view.querySelector(`label[for="${element.id}"]`)).not.toBeNull();
    }
    expect(view.textContent).toContain('设备选择器');
    expect(view.textContent).toContain('Source 选择器');
    expect(view.textContent).toContain('工具 ID');
    expect(view.textContent).toContain('Skills');
    expect(view.textContent).toContain('审批策略');
    expect(view.textContent).toContain('证据策略');
  });

  it('summarises the current revision and the enable state without imply saving enables it', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });

    const summary = view.querySelector<HTMLElement>('[data-group="summary"]')!;
    expect(summary.textContent).toContain('版本');
    expect(summary.textContent).toContain('v3');
    expect(summary.textContent).toContain('已启用');
    expect(summary.textContent).toContain('保存不会改变启停状态');
  });

  it('saves a new blank workflow as disabled and never sends enabled', async () => {
    await mount();
    type('workflow-objective', '新建巡检');
    type('workflow-trigger-id', 't-new');
    type('workflow-cron-expression', '0 * * * *');
    createWorkflow.mockResolvedValueOnce(
      response(
        revision({
          workflowId: 'wf:new',
          version: 1,
          enabled: false,
          definition: definition({ objective: '新建巡检' }),
        }),
      ),
    );

    click('button[data-action="save"]');
    await settle();

    expect(createWorkflow).toHaveBeenCalledOnce();
    const sent = createWorkflow.mock.calls[0][1];
    expect(sent.objective).toBe('新建巡检');
    expect('enabled' in sent).toBe(false);
    expect(view.textContent).toContain('已保存版本 v1');
    expect(view.textContent).toContain('未启用');
    expect(window.location.hash).toBe('#/harness/workflows/wf%3Anew');
  });

  it('preserves enabled when editing and never toggles it implicitly', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    type('workflow-objective', '巡检二号泵');
    updateWorkflow.mockResolvedValueOnce(
      response(revision({ version: 4, enabled: true, definition: definition({ objective: '巡检二号泵' }) })),
    );

    click('button[data-action="save"]');
    await settle();

    expect(updateWorkflow).toHaveBeenCalledOnce();
    expect(updateWorkflow.mock.calls[0][2]).toMatchObject({
      expectedVersion: 3,
      enabled: true,
    });
    expect(setEnabled).not.toHaveBeenCalled();
    expect(view.querySelector('[data-status="enabled"]')).not.toBeNull();
    expect(view.textContent).toContain('已保存版本 v4');
    expect(view.textContent).toContain('已启用');
  });

  it('keeps local edits on stale submit and requires a manual reload', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    type('workflow-objective', '本地尚未保存的目标');
    updateWorkflow.mockRejectedValueOnce(
      new ApiError(409, 'conflict', { code: 409, result: { currentVersion: 7 } }),
    );

    click('button[data-action="save"]');
    await settle();

    expect(field('workflow-objective').value).toBe('本地尚未保存的目标');
    expect(view.textContent).toContain(WORKFLOW_CONFLICT_MESSAGE);
    expect(view.textContent).toContain('最新版本：v7');
    expect(getWorkflow).toHaveBeenCalledOnce();
    expect(view.textContent).not.toContain('差异');

    getWorkflow.mockResolvedValueOnce(
      response(revision({ version: 7, definition: definition({ objective: '服务器目标' }) })),
    );
    click('button[data-action="reload"]');
    await settle();

    expect(getWorkflow).toHaveBeenCalledTimes(2);
    expect(field('workflow-objective').value).toBe('服务器目标');
    expect(view.textContent).toContain('已重新加载版本 v7');
  });

  it('maps server validation failures to a locatable top summary and field errors', async () => {
    const details = workflowFieldErrors(
      definition(),
      new ApiError(400, 'workflow objective must not be blank'),
    );
    expect(details).toMatchObject({ objective: '目标不能为空' });
    expect(
      workflowFieldErrors(definition(), new ApiError(400, 'invalid workflow cron: bad expression')),
    ).toMatchObject({ 'trigger.expression': 'Cron 表达式无效' });

    await mount({ workflowId: 'wf:1', loaded: revision() });
    updateWorkflow.mockRejectedValueOnce(
      new ApiError(400, 'invalid', {
        code: 400,
        fields: { objective: '目标不能为空' },
      }),
    );
    click('button[data-action="save"]');
    await settle();

    const summary = view.querySelector<HTMLElement>('#harness-error-summary')!;
    expect(summary.getAttribute('role')).toBe('alert');
    expect(summary.textContent).toContain('目标不能为空');
    expect(field('workflow-objective').getAttribute('aria-invalid')).toBe('true');
    expect(view.querySelector('#workflow-objective-error')?.textContent).toContain('目标不能为空');

    view.querySelector<HTMLElement>('[data-error-field="objective"]')!.click();
    await settle();
    expect(document.activeElement).toBe(field('workflow-objective'));
  });

  it('disables only the save action while pending and keeps the page readable', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    const pending = deferred<ApiResponse<WorkflowRevision>>();
    updateWorkflow.mockReturnValueOnce(pending.promise);

    click('button[data-action="save"]');
    await settle();

    const save = view.querySelector<HTMLButtonElement>('button[data-action="save"]')!;
    expect(save.disabled).toBe(true);
    expect(save.textContent).toContain('保存中');
    expect(field('workflow-objective').disabled).toBe(false);
    expect(view.querySelector<HTMLButtonElement>('button[data-action="toggle-enabled"]')?.disabled).toBe(
      false,
    );
    expect(view.querySelector('[role="progressbar"]')).toBeNull();
    expect(view.querySelector('toast-container')).toBeNull();
    expect(view.textContent).toContain('正在保存配置');

    pending.resolve(response(revision({ version: 4 })));
    await settle();

    expect(save.disabled).toBe(false);
    expect(view.textContent).toContain('已保存版本 v4');
  });

  it('shows persistent errors with an actionable next step', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    updateWorkflow.mockRejectedValueOnce(new ApiError(503, '服务暂不可用'));

    click('button[data-action="save"]');
    await settle();

    const feedback = view.querySelector<HTMLElement>('[data-feedback="error"]')!;
    expect(feedback.getAttribute('role')).toBe('alert');
    expect(feedback.textContent).toContain('服务暂不可用');
    expect(feedback.textContent).toContain('下一步');
  });

  it('warns before leaving with unsaved edits and restores focus on cancel', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    type('workflow-objective', '未保存的更改');

    const outside = document.createElement('a');
    outside.className = 'harness-test-outside';
    outside.href = '#/harness';
    outside.textContent = '离开';
    document.body.append(outside);

    outside.click();
    await settle();
    expect(window.location.hash).toBe('');
    const dialog = view.querySelector<HTMLElement>('[role="dialog"][data-dialog="leave"]')!;
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    expect(dialog.textContent).toContain('未保存');

    click('button[data-action="leave-cancel"]');
    await settle();
    expect(document.activeElement).toBe(outside);

    const event = new Event('beforeunload', { cancelable: true });
    expect(window.dispatchEvent(event)).toBe(false);

    outside.click();
    await settle();
    click('button[data-action="leave-confirm"]');
    await settle();
    expect(window.location.hash).toBe('#/harness');
  });

  it('disables scheduling only through an explicit confirmed action', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    const pending = deferred<ApiResponse<WorkflowRevision>>();
    setEnabled.mockReturnValueOnce(pending.promise);

    click('button[data-action="toggle-enabled"]');
    await settle();
    expect(view.querySelector('[role="dialog"][data-dialog="toggle"]')).not.toBeNull();
    expect(view.textContent).toContain('停用后只停止未来计划');

    click('button[data-action="toggle-confirm"]');
    await settle();
    expect(setEnabled).toHaveBeenCalledWith('ws_1', 'wf:1', false);
    expect(view.querySelector<HTMLButtonElement>('button[data-action="toggle-enabled"]')?.disabled).toBe(
      true,
    );
    expect(view.querySelector<HTMLButtonElement>('button[data-action="save"]')?.disabled).toBe(false);

    pending.resolve(response(revision({ enabled: false })));
    await settle();

    expect(view.querySelector('[data-status="disabled"]')).not.toBeNull();
    expect(view.textContent).toContain('已停用计划');
    expect(updateWorkflow).not.toHaveBeenCalled();
  });
});

describe('Harness workflow editor interactions', () => {
  it('focuses the route H1 after loading', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });

    const heading = view.querySelector('h1');
    expect(heading).not.toBeNull();
    expect(heading?.getAttribute('tabindex')).toBe('-1');
    expect(document.activeElement).toBe(heading);
  });

  it('supports arrow-key navigation across trigger tabs', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    const tabs = [...view.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    const cron = view.querySelector<HTMLButtonElement>('[role="tab"][data-trigger-type="cron"]')!;
    const watch = view.querySelector<HTMLButtonElement>('[role="tab"][data-trigger-type="watch"]')!;
    const event = view.querySelector<HTMLButtonElement>('[role="tab"][data-trigger-type="event"]')!;

    expect(tabs).toHaveLength(3);
    expect(cron.getAttribute('aria-selected')).toBe('true');
    expect(cron.tabIndex).toBe(0);
    expect(watch.tabIndex).toBe(-1);

    cron.focus();
    cron.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    await settle();
    expect(watch.getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(watch);

    watch.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
    await settle();
    expect(event.getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(event);

    event.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
    await settle();
    expect(cron.getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(cron);
  });

  it('uses a keyboard-accessible overflow menu and returns focus on Escape', async () => {
    await mount({ workflowId: 'wf:1', loaded: revision() });
    const trigger = view.querySelector<HTMLButtonElement>('button[data-action="more"]')!;
    trigger.focus();
    trigger.click();
    await settle();

    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    const items = [...view.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
    expect(items.length).toBeGreaterThan(0);
    expect(document.activeElement).toBe(items[0]);

    items[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(document.activeElement).toBe(trigger);
  });

  it('blocks save with required-field messages before calling the API', async () => {
    await mount();
    click('button[data-action="save"]');
    await settle();

    expect(createWorkflow).not.toHaveBeenCalled();
    expect(view.querySelector('#harness-error-summary')?.textContent).toContain('请输入目标');
    expect(view.querySelector('#workflow-objective-error')?.textContent).toContain('请输入目标');
    expect(document.activeElement).toBe(field('workflow-objective'));
  });
});
