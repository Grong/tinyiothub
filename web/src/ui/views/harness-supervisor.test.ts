import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import { getHarnessSupervisor, retryHarnessDeadWake } from '../../api/harness.js';
import type {
  HarnessDeadWake,
  HarnessSupervisor,
  HarnessSupervisorHealth,
} from '../../types/harness.js';
import './harness-supervisor.js';
import type { HarnessSupervisorView } from './harness-supervisor.js';

vi.mock('../../api/harness.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/harness.js')>();
  return {
    ...actual,
    getHarnessSupervisor: vi.fn(),
    retryHarnessDeadWake: vi.fn(),
  };
});

const getSupervisor = vi.mocked(getHarnessSupervisor);
const retryDeadWake = vi.mocked(retryHarnessDeadWake);
let view: HarnessSupervisorView;

function health(overrides: Partial<HarnessSupervisorHealth> = {}): HarnessSupervisorHealth {
  return {
    status: 'healthy',
    stalledReason: null,
    stalledLabel: null,
    tickSeq: 42,
    lastStartedAt: '2026-11-03T10:00:00Z',
    lastCompletedAt: '2026-11-03T10:00:00Z',
    nextWakeAt: '2026-11-03T10:05:00Z',
    oldestPendingAt: null,
    schedulingDelaySeconds: 0,
    activeMains: 1,
    activeSubagents: 2,
    enabledWorkflows: 1,
    pendingWakes: 0,
    lastError: null,
    noNewRuns: false,
    observedAt: '2026-11-03T10:00:01Z',
    ...overrides,
  };
}

function deadWake(overrides: Partial<HarnessDeadWake> = {}): HarnessDeadWake {
  return {
    eventId: 'event-original',
    taskId: 'task-original',
    failureReason: 'provider timeout',
    createdAt: '2026-11-03T09:55:00Z',
    retriedBy: null,
    retriedAt: null,
    ...overrides,
  };
}

function payload(overrides: Partial<HarnessSupervisor> = {}): HarnessSupervisor {
  return {
    health: health(),
    deadWakes: [deadWake()],
    canRetry: true,
    readOnly: false,
    ...overrides,
  };
}

function response(result: HarnessSupervisor) {
  return { code: 0, msg: '', result };
}

function retryResponse(eventId = 'event-original') {
  return {
    code: 0,
    msg: '',
    result: { eventId, state: 'pending', retriedBy: 'admin-user' },
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

async function mount(result: HarnessSupervisor = payload()): Promise<void> {
  getSupervisor.mockResolvedValue(response(result));
  view = document.createElement('view-harness-supervisor') as HarnessSupervisorView;
  view.workspaceId = 'ws_1';
  document.body.append(view);
  await settle();
}

function retryButton(): HTMLButtonElement | null {
  return view.querySelector<HTMLButtonElement>('[data-dead-wake="event-original"] button');
}

beforeEach(() => {
  document.body.innerHTML = '';
  getSupervisor.mockReset();
  retryDeadWake.mockReset();
});

afterEach(() => {
  view?.remove();
});

describe('harness supervisor health', () => {
  it('distinguishes a normal idle scheduler from a stalled one', async () => {
    await mount(
      payload({
        health: health({
          status: 'idle',
          noNewRuns: true,
          enabledWorkflows: 0,
          nextWakeAt: null,
          activeMains: 0,
          activeSubagents: 0,
        }),
      }),
    );
    expect(view.textContent).toContain('正常空闲');
    expect(view.textContent).toContain('当前没有新的 Run');
    expect(view.textContent).not.toContain('调度停滞');

    view.remove();
    await mount(
      payload({
        health: health({
          status: 'stalled',
          stalledReason: 'due_not_started',
          stalledLabel: '调度器停滞：到期未开始',
          noNewRuns: true,
        }),
      }),
    );
    expect(view.textContent).toContain('调度停滞');
    expect(view.textContent).toContain('到期未开始');
    expect(view.textContent).not.toContain('当前没有新的 Run');
  });

  it('shows bounded scheduler facts without inventing liveness', async () => {
    await mount(
      payload({
        health: health({
          nextWakeAt: '2026-11-03T10:05:00Z',
          oldestPendingAt: '2026-11-03T09:58:00Z',
          schedulingDelaySeconds: 125,
          activeMains: 3,
          activeSubagents: 7,
          lastError: 'provider timeout',
        }),
      }),
    );

    const facts = view.querySelector('[data-health-facts]');
    expect(facts?.textContent).toContain('下一次计划唤醒');
    expect(facts?.textContent).toContain('计划时间，不承诺准点');
    expect(facts?.textContent).toContain('最早已等待');
    expect(facts?.textContent).toContain('调度延迟');
    expect(facts?.textContent).toContain('125 秒');
    expect(facts?.textContent).toContain('活动 Main');
    expect(facts?.textContent).toContain('3');
    expect(facts?.textContent).toContain('活动 Subagent');
    expect(facts?.textContent).toContain('7');
    expect(facts?.textContent).toContain('最近错误');
    expect(facts?.textContent).toContain('provider timeout');
  });

  it('lists each dead wake with its original identity and retries only that event', async () => {
    retryDeadWake.mockResolvedValue(retryResponse());
    await mount();
    const row = view.querySelector('[data-dead-wake="event-original"]');
    expect(row?.textContent).toContain('event-original');
    expect(row?.textContent).toContain('task-original');
    expect(row?.textContent).toContain('provider timeout');

    const button = retryButton();
    expect(button).not.toBeNull();
    button?.click();
    expect(retryDeadWake).toHaveBeenCalledWith('ws_1', 'event-original');
    await settle();

    expect(view.textContent).toContain('已按原事件 event-original 重新排队');
    expect(button?.disabled).toBe(false);
    expect(view.querySelector('[data-dead-wake="event-original"]')).not.toBeNull();
  });

  it('disables only the selected row while the retry request is in flight', async () => {
    let release: (value: ReturnType<typeof retryResponse>) => void = () => {};
    retryDeadWake.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    await mount(
      payload({
        deadWakes: [deadWake(), deadWake({ eventId: 'event-other', taskId: 'task-other' })],
      }),
    );

    retryButton()?.click();
    await settle();
    expect(retryButton()?.disabled).toBe(true);
    expect(retryButton()?.textContent).toContain('重试中');
    const other = view.querySelector<HTMLButtonElement>('[data-dead-wake="event-other"] button');
    expect(other?.disabled).toBe(false);

    release(retryResponse());
    await settle();
    expect(retryButton()?.disabled).toBe(false);
  });

  it('keeps a persistent inline error and re-enables the row after failure', async () => {
    retryDeadWake.mockRejectedValue(new ApiError(403, '需要管理员权限'));
    await mount();
    retryButton()?.click();
    await settle();

    const alert = view.querySelector('[data-dead-wake="event-original"] [role="alert"]');
    expect(alert?.textContent).toContain('需要管理员权限');
    expect(retryButton()?.disabled).toBe(false);
    expect(view.textContent).toContain('下一页操作：确认权限后重试该事件。');
  });

  it('is read-only without Agent management permission', async () => {
    await mount(payload({ canRetry: false, readOnly: true }));
    expect(retryButton()).toBeNull();
    expect(view.textContent).toContain('只读');
    expect(view.textContent).toContain('需要 Agent 管理权限');
  });

  it('has an explicit empty state and a load error state', async () => {
    await mount(payload({ deadWakes: [] }));
    expect(view.textContent).toContain('没有待恢复的 dead wake');

    view.remove();
    getSupervisor.mockRejectedValue(new ApiError(503, 'Harness 存储不可用'));
    view = document.createElement('view-harness-supervisor') as HarnessSupervisorView;
    view.workspaceId = 'ws_1';
    document.body.append(view);
    await settle();
    expect(view.querySelector('[role="alert"]')?.textContent).toContain('Harness 存储不可用');
  });
});
