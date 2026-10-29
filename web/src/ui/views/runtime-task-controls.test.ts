import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client.js';
import { cancelRuntimeTask, pauseRuntimeTask, resumeRuntimeTask } from '../../api/runtime.js';
import type {
  RuntimeJob,
  RuntimeTask,
  RuntimeWatch,
  TaskProjection,
  TaskStatus,
} from '../../types/runtime.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-task-controls.js';
import type { RuntimeTaskControls } from './runtime-task-controls.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return {
    ...actual,
    pauseRuntimeTask: vi.fn(),
    resumeRuntimeTask: vi.fn(),
    cancelRuntimeTask: vi.fn(),
  };
});

const pause = vi.mocked(pauseRuntimeTask);
const resume = vi.mocked(resumeRuntimeTask);
const cancel = vi.mocked(cancelRuntimeTask);

function task(id = 'task_1', status: TaskStatus = 'active', over: Partial<RuntimeTask> = {}): RuntimeTask {
  return {
    id, workspaceId: 'ws_1', idempotencyKey: `key_${id}`, objective: `Objective ${id}`,
    sessionKey: null, createdBy: 'user_1', status, currentRunId: 'run_1', stateVersion: 0,
    createdAt: '2026-10-27T10:00:00Z', updatedAt: '2026-10-27T10:00:00Z', endedAt: null,
    ...over,
  };
}

function job(over: Partial<RuntimeJob> = {}): RuntimeJob {
  return {
    id: 'job_1', workspaceId: 'ws_1', deviceId: 'dev_1', tool: 'set_device',
    argsJson: { value: 21 }, state: 'running', stopRequestedAt: null,
    progressJson: null, resultJson: null, taskId: 'task_1', runId: 'run_1',
    toolCallId: 'call_1', driverCallId: null, timeoutMs: 1000,
    startedAt: '2026-10-27T10:00:00Z', endedAt: null,
    ...over,
  };
}

function watch(id = 'watch_1'): RuntimeWatch {
  return {
    id, condition: { thingId: 'dev_watch' }, persistent: true, timeoutSecs: 30,
    createdAt: '2026-10-27T10:00:00Z',
  };
}

function projectionSnapshot(
  status: TaskStatus,
  options: { jobs?: RuntimeJob[]; watches?: RuntimeWatch[]; taskId?: string } = {},
): TaskProjection {
  const id = options.taskId ?? 'task_1';
  const jobs = options.jobs ?? [];
  const watches = options.watches ?? [];
  return {
    task: task(id, status),
    runs: [], jobs, approvals: [], watches, evidence: [], summary: null, cursor: 0,
    associations: jobs.map((item) => ({ deviceId: item.deviceId, sourceIds: [] })),
    devices: [],
  };
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void } {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function ok(result: TaskProjection) {
  return { code: 0, msg: '', result };
}

interface ControlsFixture {
  readonly element: RuntimeTaskControls;
  readonly projection: RuntimeProjection;
  readonly text: string;
  readonly pauseButton: HTMLButtonElement | null;
  readonly resumeButton: HTMLButtonElement | null;
  readonly primaryButton: HTMLButtonElement | null;
  readonly dangerButton: HTMLButtonElement | null;
  readonly cancelDialogOpen: boolean;
  readonly executableControls: HTMLElement[];
  readonly emitted: TaskProjection[];
  readonly openCancel: () => Promise<void>;
}

async function controlsFixture(snapshot: TaskProjection): Promise<ControlsFixture> {
  const projection = new RuntimeProjection(snapshot);
  const element = document.createElement('runtime-task-controls') as RuntimeTaskControls;
  element.workspaceId = snapshot.task.workspaceId;
  element.taskId = snapshot.task.id;
  element.projection = projection;
  const emitted: TaskProjection[] = [];
  element.addEventListener('projection-change', (event) => {
    emitted.push((event as CustomEvent<TaskProjection>).detail);
  });
  document.body.append(element);
  await element.updateComplete;
  return {
    element,
    projection,
    get text() { return element.textContent ?? ''; },
    get pauseButton() { return element.querySelector<HTMLButtonElement>('button[data-action="pause"]'); },
    get resumeButton() { return element.querySelector<HTMLButtonElement>('button[data-action="resume"]'); },
    get primaryButton() { return element.querySelector<HTMLButtonElement>('button[data-action="primary"]'); },
    get dangerButton() { return element.querySelector<HTMLButtonElement>('button[data-action="danger"]'); },
    get cancelDialogOpen() { return element.querySelector('runtime-confirm-dialog') !== null; },
    get executableControls() { return [...element.querySelectorAll<HTMLElement>('[data-executable]')]; },
    emitted,
    openCancel: async () => {
      element.querySelector<HTMLButtonElement>('button[data-action="overflow"]')!.click();
      await element.updateComplete;
      element.querySelector<HTMLButtonElement>('button[data-action="menu-cancel"]')!.click();
      await element.updateComplete;
    },
  };
}

async function flush(element: RuntimeTaskControls): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await element.updateComplete;
}

beforeEach(() => {
  pause.mockReset();
  resume.mockReset();
  cancel.mockReset();
});

afterEach(() => {
  document.body.innerHTML = '';
  vi.clearAllMocks();
});

describe('runtime-task-controls', () => {
  it('pause submits directly and explains active work continues', async () => {
    const response = deferred<{ code: number; msg: string; result: TaskProjection }>();
    pause.mockReturnValue(response.promise);
    const controls = await controlsFixture(projectionSnapshot('active', { jobs: [job()] }));

    controls.pauseButton!.click();
    await controls.element.updateComplete;

    expect(pause).toHaveBeenCalledOnce();
    expect(pause).toHaveBeenCalledWith('ws_1', 'task_1', expect.any(AbortSignal));
    expect(controls.cancelDialogOpen).toBe(false);
    expect(controls.text).toContain('Stops new runs; active jobs and watches continue');
    expect(controls.text).toContain('Pausing…');
    expect(controls.pauseButton!.disabled).toBe(true);

    response.resolve(ok(projectionSnapshot('paused', { jobs: [job()] })));
    await flush(controls.element);

    expect(controls.emitted).toHaveLength(1);
    expect(controls.emitted[0].task.status).toBe('paused');
    expect(controls.text).not.toContain('Pausing…');
  });

  it('resume submits directly with a pending label and disables repeats', async () => {
    const response = deferred<{ code: number; msg: string; result: TaskProjection }>();
    resume.mockReturnValue(response.promise);
    const controls = await controlsFixture(projectionSnapshot('paused'));

    controls.resumeButton!.click();
    controls.resumeButton!.click();
    await controls.element.updateComplete;

    expect(resume).toHaveBeenCalledOnce();
    expect(controls.text).toContain('Resuming…');
    expect(controls.resumeButton!.disabled).toBe(true);

    response.resolve(ok(projectionSnapshot('active')));
    await flush(controls.element);
    expect(controls.emitted[0].task.status).toBe('active');
  });

  it('keeps the original projection and shows an error when pause fails', async () => {
    pause.mockRejectedValue(new ApiError(503, 'Pause unavailable'));
    const controls = await controlsFixture(projectionSnapshot('active'));

    controls.pauseButton!.click();
    await flush(controls.element);

    expect(controls.text).toContain('Pause unavailable');
    expect(controls.projection.task.status).toBe('active');
    expect(controls.emitted).toEqual([]);
    expect(controls.pauseButton!.disabled).toBe(false);
  });

  it('cancel requires confirmation and defaults to keep running', async () => {
    const controls = await controlsFixture(projectionSnapshot('active', { jobs: [job()], watches: [watch()] }));

    await controls.openCancel();
    expect(cancel).not.toHaveBeenCalled();
    expect(controls.primaryButton!.textContent).toContain('Keep running');
    controls.primaryButton!.click();
    await controls.element.updateComplete;
    expect(cancel).not.toHaveBeenCalled();
    expect(controls.cancelDialogOpen).toBe(false);

    await controls.openCancel();
    controls.dangerButton!.click();
    expect(cancel).toHaveBeenCalledOnce();
  });

  it('cancel dialog names the objective, active work, devices and unknown-result risk', async () => {
    const controls = await controlsFixture(projectionSnapshot('active', {
      jobs: [job({ id: 'job_live', deviceId: 'dev_live', state: 'running' }), job({ id: 'job_done', deviceId: 'dev_done', state: 'done' })],
      watches: [watch()],
    }));

    await controls.openCancel();

    expect(controls.text).toContain('Objective task_1');
    expect(controls.text).toContain('1 active job');
    expect(controls.text).toContain('1 active watch');
    expect(controls.text).toContain('dev_live');
    expect(controls.text.toLowerCase()).toContain('unknown');
    expect(controls.text).toContain('Keep running');
  });

  it('submits one cancel request even for repeated danger clicks and keeps Cancelling after close', async () => {
    const response = deferred<{ code: number; msg: string; result: TaskProjection }>();
    cancel.mockReturnValue(response.promise);
    const controls = await controlsFixture(projectionSnapshot('active', { jobs: [job()] }));

    await controls.openCancel();
    const danger = controls.dangerButton!;
    danger.click();
    danger.click();
    await controls.element.updateComplete;

    expect(cancel).toHaveBeenCalledOnce();
    expect(controls.cancelDialogOpen).toBe(false);
    expect(controls.text).toContain('Cancelling');

    response.resolve(ok(projectionSnapshot('cancelling', { jobs: [job()] })));
    await flush(controls.element);
    expect(controls.text).toContain('Cancelling');
    expect(controls.emitted[0].task.status).toBe('cancelling');
  });

  it('restores a failed cancel to the original state with a visible error', async () => {
    cancel.mockRejectedValue(new ApiError(503, 'Cancel unavailable'));
    const controls = await controlsFixture(projectionSnapshot('active'));

    await controls.openCancel();
    controls.dangerButton!.click();
    await flush(controls.element);

    expect(controls.text).toContain('Cancel unavailable');
    expect(controls.text).not.toContain('Cancelling');
    expect(controls.cancelDialogOpen).toBe(false);
    expect(controls.executableControls.length).toBeGreaterThan(0);
  });

  it('aborts an in-flight control request and clears pending state on Task switch', async () => {
    const response = deferred<{ code: number; msg: string; result: TaskProjection }>();
    pause.mockReturnValue(response.promise);
    const controls = await controlsFixture(projectionSnapshot('active'));

    controls.pauseButton!.click();
    await controls.element.updateComplete;
    const signal = pause.mock.calls[0][2] as AbortSignal;
    expect(signal.aborted).toBe(false);

    controls.element.taskId = 'task_2';
    await controls.element.updateComplete;

    expect(signal.aborted).toBe(true);
    expect(controls.text).not.toContain('Pausing…');
    expect(controls.text).not.toContain('Cancelling');
    expect(controls.cancelDialogOpen).toBe(false);
  });

  it('renders no executable controls for history or terminal Tasks', async () => {
    const done = await controlsFixture(projectionSnapshot('completed', { jobs: [job({ state: 'done' })] }));
    expect(done.executableControls).toEqual([]);
    expect(done.pauseButton).toBeNull();
    expect(done.resumeButton).toBeNull();

    const history = await controlsFixture(projectionSnapshot('active', { jobs: [job()] }));
    history.element.readOnly = true;
    await history.element.updateComplete;
    expect(history.executableControls).toEqual([]);
    expect(history.pauseButton).toBeNull();
  });

  it('renders cancellation attention with affected devices and an explicit refresh', async () => {
    const controls = await controlsFixture(projectionSnapshot('failed', {
      jobs: [
        job({ id: 'job_unknown_a', deviceId: 'dev_a', state: 'cancelled_unknown' }),
        job({ id: 'job_unknown_b', deviceId: 'dev_b', state: 'cancelled_unknown' }),
      ],
    }));
    const refresh = vi.fn();
    controls.element.addEventListener('refresh-request', refresh);

    expect(controls.text).toContain('Cancellation needs attention');
    expect(controls.text).toContain('dev_a');
    expect(controls.text).toContain('dev_b');
    expect(controls.text).not.toContain('All stopped');
    const button = controls.element.querySelector<HTMLButtonElement>('button[data-action="refresh-status"]')!;
    expect(button.textContent).toContain('Refresh status');
    button.click();
    expect(refresh).toHaveBeenCalledOnce();
    expect(controls.executableControls).toEqual([]);
  });
});
