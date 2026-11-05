import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import {
  instantiateHarnessWorkflowTemplate,
  listHarnessWorkflowTemplates,
} from '../../api/harness.js';
import type {
  HarnessWorkflowTemplate,
  WorkflowDefinition,
  WorkflowRevision,
} from '../../types/harness.js';
import './harness-templates.js';
import type { HarnessTemplatesView } from './harness-templates.js';

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    listHarnessWorkflowTemplates: vi.fn(),
    instantiateHarnessWorkflowTemplate: vi.fn(),
  };
});

const listTemplates = vi.mocked(listHarnessWorkflowTemplates);
const instantiate = vi.mocked(instantiateHarnessWorkflowTemplate);
let view: HarnessTemplatesView;

function definition(
  overrides: Partial<WorkflowDefinition> = {},
): WorkflowDefinition {
  return {
    objective: '检查设备状态',
    trigger: { type: 'cron', triggerId: 'daily', expression: '0 9 * * *' },
    deviceSelector: {},
    sourceSelector: { sources: ['telemetry'] },
    toolIds: ['read_properties'],
    skillRefs: ['device-management'],
    requiredCapabilities: ['text'],
    approvalPolicy: {},
    evidencePolicy: { required: true, kinds: ['property_snapshot'] },
    ...overrides,
  };
}

function template(
  id: string,
  title: string,
  overrides: Partial<HarnessWorkflowTemplate> = {},
): HarnessWorkflowTemplate {
  return {
    id,
    version: 1,
    title,
    description: `${title}说明`,
    definition: definition(),
    ...overrides,
  };
}

const templates: HarnessWorkflowTemplate[] = [
  template('inspection', '巡检'),
  template('video', '视频', {
    definition: definition({
      sourceSelector: { sources: ['front'] },
      toolIds: ['read_device'],
      requiredCapabilities: ['vision', 'video'],
      evidencePolicy: { required: true, kinds: ['media'] },
    }),
  }),
  template('energy', '节能', {
    definition: definition({
      toolIds: ['read_properties', 'send_command'],
      approvalPolicy: { mandatory: true, timeoutSecs: 120, reason: '控制需要审批' },
    }),
  }),
];

function revision(overrides: Partial<WorkflowRevision> = {}): WorkflowRevision {
  return {
    workflowId: 'workflow:1',
    version: 1,
    enabled: true,
    definition: definition({ deviceSelector: { device_ids: ['pump-1'] } }),
    createdAt: '2026-11-03T10:00:00Z',
    nextWakeAt: '2026-11-04T01:00:00Z',
    ...overrides,
  };
}

function response<T>(result: T) {
  return { code: 0, msg: '', result };
}

async function settle(): Promise<void> {
  for (let index = 0; index < 5; index += 1) {
    await Promise.resolve();
    await view.updateComplete;
  }
  await new Promise((resolve) => setTimeout(resolve, 0));
  await view.updateComplete;
}

async function mount(): Promise<void> {
  listTemplates.mockResolvedValue(response({ templates }));
  view = document.createElement('view-harness-templates') as HarnessTemplatesView;
  view.workspaceId = 'ws_1';
  document.body.append(view);
  await settle();
}

function click(selector: string): void {
  const element = view.querySelector<HTMLElement>(selector);
  if (!element) throw new Error(`Missing control ${selector}`);
  element.click();
}

function field(id: string): HTMLTextAreaElement {
  const element = view.querySelector<HTMLTextAreaElement>(`#${id}`);
  if (!element) throw new Error(`Missing field ${id}`);
  return element;
}

function type(id: string, value: string): void {
  const element = field(id);
  element.value = value;
  element.dispatchEvent(new Event('input', { bubbles: true }));
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/dashboard');
  listTemplates.mockReset();
  instantiate.mockReset();
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.clearAllMocks();
});

describe('Harness workflow template journey', () => {
  it('renders the three versioned templates and keeps dry-run optional', async () => {
    await mount();

    const cards = [...view.querySelectorAll<HTMLElement>('[data-template-id]')];
    expect(cards.map((card) => card.dataset.templateId)).toEqual([
      'inspection',
      'video',
      'energy',
    ]);
    expect(view.textContent).toContain('巡检');
    expect(view.textContent).toContain('视频');
    expect(view.textContent).toContain('节能');
    expect(view.textContent).toContain('v1');
    expect(view.textContent).toContain('可选 dry-run');
    expect(view.textContent).toContain('不是启用的前置步骤');
    expect(view.querySelector('[data-dry-run-optional]')).not.toBeNull();
    // No template is preselected, so creating is not possible until a choice.
    expect(view.querySelector('[data-action="instantiate"]')).toBeNull();
  });

  it('selects the video template and exposes its source and vision requirement', async () => {
    await mount();
    click('[data-template-id="video"]');
    await settle();

    expect(view.textContent).toContain('vision');
    expect(field('template-source-selector').value).toContain('front');
    expect(
      view.querySelector('[data-template-id="video"]')?.getAttribute('aria-pressed'),
    ).toBe('true');
    expect(
      [...view.querySelectorAll('label')].map((label) => label.getAttribute('for')),
    ).toEqual(
      expect.arrayContaining([
        'template-device-selector',
        'template-source-selector',
        'template-approval-policy',
      ]),
    );
  });

  it('creates an enabled workflow without a dry-run and shows version and next wake', async () => {
    instantiate.mockResolvedValueOnce(response(revision()));
    await mount();
    click('[data-template-id="inspection"]');
    await settle();
    type('template-device-selector', '{"device_ids":["pump-1"]}');
    click('[data-action="instantiate"]');
    await settle();

    expect(instantiate).toHaveBeenCalledTimes(1);
    expect(instantiate).toHaveBeenCalledWith('ws_1', 'inspection', {
      workflowId: '',
      deviceSelector: { device_ids: ['pump-1'] },
      sourceSelector: { sources: ['telemetry'] },
      approvalPolicy: {},
    });
    const result = view.querySelector<HTMLElement>('[data-template-result]');
    expect(result?.textContent).toContain('已启用');
    expect(result?.textContent).toContain('版本 1');
    expect(result?.textContent).toContain('下一次计划 wake');
    const detail = result?.querySelector<HTMLAnchorElement>('a');
    expect(detail?.getAttribute('href')).toBe('#/harness/workflows/workflow%3A1');
  });

  it('rejects invalid JSON and empty device selectors without calling the API', async () => {
    await mount();
    click('[data-template-id="inspection"]');
    await settle();
    type('template-device-selector', 'not json');
    click('[data-action="instantiate"]');
    await settle();

    expect(instantiate).not.toHaveBeenCalled();
    expect(view.querySelector('[data-field-error="deviceSelector"]')?.textContent).toContain(
      '不是有效 JSON',
    );

    type('template-device-selector', '{"device_ids":[]}');
    click('[data-action="instantiate"]');
    await settle();
    expect(instantiate).not.toHaveBeenCalled();
    expect(view.querySelector('[data-field-error="deviceSelector"]')?.textContent).toContain(
      '至少一个设备',
    );
  });

  it('shows a persistent validation error and keeps the form editable', async () => {
    instantiate.mockRejectedValueOnce(new ApiError(400, '设备不存在: pump-9'));
    await mount();
    click('[data-template-id="inspection"]');
    await settle();
    type('template-device-selector', '{"device_ids":["pump-9"]}');
    click('[data-action="instantiate"]');
    await settle();

    expect(view.querySelector('[role="alert"]')?.textContent).toContain('设备不存在');
    expect(view.querySelector('[data-template-result]')).toBeNull();
    expect(field('template-device-selector').value).toContain('pump-9');
  });

  it('maps a failed template catalog load to a retryable error', async () => {
    listTemplates.mockRejectedValueOnce(new ApiError(503, '服务暂不可用'));
    view = document.createElement('view-harness-templates') as HarnessTemplatesView;
    view.workspaceId = 'ws_1';
    document.body.append(view);
    await settle();

    expect(view.querySelector('[role="alert"]')?.textContent).toContain('服务暂不可用');
    const retry = view.querySelector<HTMLButtonElement>('[data-action="retry"]');
    expect(retry).not.toBeNull();
    listTemplates.mockResolvedValueOnce(response({ templates }));
    retry!.click();
    await settle();
    expect(view.querySelectorAll('[data-template-id]')).toHaveLength(3);
  });
});
