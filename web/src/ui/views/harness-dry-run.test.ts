import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import {
  getHarnessSimulation,
  listHarnessWorkflowSnapshots,
  startHarnessDryRun,
} from '../../api/harness.js';
import type {
  HarnessSimulationProjection,
  HarnessWorkflowSnapshot,
} from '../../types/harness.js';
import './harness-dry-run.js';
import type { HarnessDryRunView } from './harness-dry-run.js';

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    listHarnessWorkflowSnapshots: vi.fn(),
    startHarnessDryRun: vi.fn(),
    getHarnessSimulation: vi.fn(),
  };
});

const listSnapshots = vi.mocked(listHarnessWorkflowSnapshots);
const startDryRun = vi.mocked(startHarnessDryRun);
const getSimulation = vi.mocked(getHarnessSimulation);
let view: HarnessDryRunView;

function response<T>(result: T) {
  return { code: 0, msg: '', result };
}

const snapshots: HarnessWorkflowSnapshot[] = [
  {
    seq: 12,
    observedAt: '2026-11-02T08:30:00Z',
    declarationHash: 'declaration-hash-1',
    driverVersion: 'driver-1.0.0',
  },
  {
    seq: 9,
    observedAt: '2026-11-02T07:30:00Z',
    declarationHash: 'declaration-hash-0',
    driverVersion: 'driver-1.0.0',
  },
];

function projection(overrides: Partial<HarnessSimulationProjection> = {}): HarnessSimulationProjection {
  return {
    executionMode: 'simulation',
    readOnly: true,
    taskId: 'task:sim',
    runId: 'run:sim',
    status: 'completed',
    workflowId: 'wf:1',
    workflowVersion: 3,
    objective: '巡检一号泵',
    snapshot: {
      seq: 12,
      observedAt: '2026-11-02T08:30:00Z',
      declarationHash: 'declaration-hash-1',
      driverVersion: 'driver-1.0.0',
    },
    steps: [
      {
        id: '12:run.started',
        name: 'run.started',
        args: { snapshotSeq: 12 },
        output: '{"snapshotSeq":12}',
      },
    ],
    evidence: ['evidence-sim-1'],
    errors: [],
    outcome: {
      status: 'completed',
      summary: '模拟运行完成',
      observations: ['temperature 20 from snapshot'],
      evidenceRefs: ['evidence-sim-1'],
      recommendedActions: ['set_speed'],
    },
    ...overrides,
  };
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
  options: {
    version?: number;
    snapshotSeq?: number | null;
    taskId?: string;
    snapshotsResult?: HarnessWorkflowSnapshot[] | Error;
    projectionResult?: HarnessSimulationProjection | Error;
  } = {},
): Promise<void> {
  if (options.snapshotsResult instanceof Error) {
    listSnapshots.mockRejectedValueOnce(options.snapshotsResult);
  } else {
    listSnapshots.mockResolvedValueOnce(
      response({
        workflowId: 'wf:1',
        version: options.version ?? 3,
        snapshots: options.snapshotsResult ?? snapshots,
      }),
    );
  }
  if (options.taskId && options.projectionResult) {
    if (options.projectionResult instanceof Error) {
      getSimulation.mockRejectedValueOnce(options.projectionResult);
    } else {
      getSimulation.mockResolvedValueOnce(response(options.projectionResult));
    }
  }
  view = document.createElement('view-harness-dry-run') as HarnessDryRunView;
  view.workspaceId = 'ws_1';
  view.workflowId = 'wf:1';
  view.version = options.version ?? 3;
  view.snapshotSeq = options.snapshotSeq ?? null;
  view.taskId = options.taskId ?? '';
  document.body.append(view);
  await settle();
}

function selectSnapshot(seq: number): void {
  const select = view.querySelector<HTMLSelectElement>('#dry-run-snapshot');
  if (!select) throw new Error('Missing snapshot picker');
  select.value = String(seq);
  select.dispatchEvent(new Event('change', { bubbles: true }));
}

function click(selector: string): void {
  const element = view.querySelector<HTMLElement>(selector);
  if (!element) throw new Error(`Missing control ${selector}`);
  element.click();
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/dashboard');
  listSnapshots.mockReset();
  startDryRun.mockReset();
  getSimulation.mockReset();
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.clearAllMocks();
});

describe('Harness isolated simulation Run page', () => {
  it('shows a persistent simulation banner and a snapshot picker with seq/observed_at', async () => {
    await mount();

    const banner = view.querySelector('[data-simulation-banner]');
    expect(banner?.textContent).toContain('模拟运行');
    expect(banner?.textContent).toContain('不代表真实模型、设备或物理结果');
    expect(view.querySelector('h1')?.textContent).toContain('模拟运行');

    const options = [...view.querySelectorAll<HTMLOptionElement>('#dry-run-snapshot option')];
    expect(options.map((option) => option.value)).toEqual(['12', '9']);
    expect(options[0].textContent).toContain('12');
    expect(options[0].textContent).toContain('2026');
    expect(options[0].textContent).toMatch(/(GMT|UTC|CST|时区)/);

    expect(view.textContent).toContain('返回此版本');
    expect(view.querySelector('[data-action="return-revision"]')?.getAttribute('href')).toBe(
      '#/harness/workflows/wf%3A1',
    );
    // No real execution or production approval entry on the simulation page.
    expect(view.textContent).not.toContain('批准真实执行');
    expect(view.textContent).not.toContain('真实执行');
    expect(view.querySelector('[data-action="approve"]')).toBeNull();
    expect(view.querySelector('[data-action="execute"]')).toBeNull();
    expect(listSnapshots).toHaveBeenCalledWith('ws_1', 'wf:1', 3, expect.any(AbortSignal));
  });

  it('starts an isolated run with the selected snapshot and renders the generic read-only projection', async () => {
    startDryRun.mockResolvedValueOnce(response({ taskId: 'task:sim', runId: 'run:sim' }));
    getSimulation.mockResolvedValueOnce(response(projection()));
    await mount();
    selectSnapshot(12);
    await settle();
    click('[data-action="start-dry-run"]');
    await settle();

    expect(startDryRun).toHaveBeenCalledWith('ws_1', 'wf:1', 3, 12);
    expect(getSimulation).toHaveBeenCalledWith('ws_1', 'task:sim', expect.any(AbortSignal));

    const result = view.querySelector('[data-simulation-result]');
    expect(result?.textContent).toContain('模拟运行');
    expect(result?.textContent).toContain('task:sim');
    expect(result?.textContent).toContain('run:sim');
    expect(result?.textContent).toContain('巡检一号泵');
    expect(result?.textContent).toContain('temperature 20 from snapshot');
    expect(result?.textContent).toContain('evidence-sim-1');

    const timeline = view.querySelector('run-timeline');
    expect(timeline).not.toBeNull();
    // Read-only: the timeline has no execution/approval controls.
    expect(timeline?.querySelector('button')).toBeNull();

    // Banner remains for the whole page after results load.
    expect(view.querySelector('[data-simulation-banner]')).not.toBeNull();
    expect(view.querySelector('[data-action="return-revision"]')?.getAttribute('href')).toBe(
      '#/harness/workflows/wf%3A1',
    );
  });

  it('keeps the picker visible and explains a missing snapshot loudly', async () => {
    await mount();
    selectSnapshot(12);
    await settle();
    startDryRun.mockRejectedValueOnce(new ApiError(404, '快照不存在: snapshot:12'));
    click('[data-action="start-dry-run"]');
    await settle();

    expect(view.querySelector('[role="alert"]')?.textContent).toContain('快照不存在');
    expect(view.querySelector('#dry-run-snapshot')).not.toBeNull();
    expect(view.querySelector('[data-simulation-result]')).toBeNull();
    expect(getSimulation).not.toHaveBeenCalled();
  });

  it('surfaces unreadable snapshot and capability errors without inventing observations', async () => {
    const failed = projection({
      status: 'failed',
      errors: [
        {
          type: 'capability.unavailable',
          message: 'capability.unavailable: tool `read_camera` requires missing capability [vision]',
          occurredAt: '2026-11-02T08:30:01Z',
        },
      ],
      outcome: {
        status: 'failed',
        summary: '缺少能力: vision',
        observations: [],
        evidenceRefs: [],
        recommendedActions: ['配置具备相应能力的 provider 后重新创建模拟 Run'],
      },
    });
    await mount({ taskId: 'task:failed', snapshotSeq: 12, projectionResult: failed });

    expect(getSimulation).toHaveBeenCalledWith('ws_1', 'task:failed', expect.any(AbortSignal));
    const result = view.querySelector('[data-simulation-result]');
    expect(result?.textContent).toContain('capability.unavailable');
    expect(result?.textContent).toContain('vision');
    // No observation was invented when the provider lacked vision.
    expect(view.querySelector('[data-observations]')?.textContent ?? '').not.toContain('temperature');
    expect(view.querySelector('[role="alert"]')).not.toBeNull();
  });

  it('shows an explicit empty state when no stamped snapshots exist', async () => {
    await mount({ snapshotsResult: [] });

    expect(view.textContent).toContain('没有可用的设备事实快照');
    const start = view.querySelector<HTMLButtonElement>('[data-action="start-dry-run"]');
    expect(start?.disabled).toBe(true);
  });
});
