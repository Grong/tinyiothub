import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { cancelRuntimeTask, getRuntimeTask, pauseRuntimeTask } from '../../api/runtime.js';
import { connectRuntimeStream, type RuntimeStreamCallbacks } from '../../api/runtime-stream.js';
import type {
  DeviceJobState,
  RuntimeEvent,
  RuntimeJob,
  TaskProjection,
  TaskStatus,
} from '../../types/runtime.js';
import type { RuntimePlayback } from './runtime-playback.js';
import type { RuntimeTaskControls } from './runtime-task-controls.js';
import { parseRuntimeHash, type RuntimeView } from './runtime-url-state.js';
import './runtime-task.js';
import type { RuntimeTaskView } from './runtime-task.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return {
    ...actual,
    getRuntimeTask: vi.fn(),
    pauseRuntimeTask: vi.fn(),
    cancelRuntimeTask: vi.fn(),
  };
});
vi.mock('../../api/runtime-stream.js', () => ({ connectRuntimeStream: vi.fn() }));

const get = vi.mocked(getRuntimeTask);
const pause = vi.mocked(pauseRuntimeTask);
const cancel = vi.mocked(cancelRuntimeTask);
const connect = vi.mocked(connectRuntimeStream);
let view: RuntimeTaskView;
let close: Mock<() => void>;
let streamCallbacks: RuntimeStreamCallbacks;

function snapshot(id = 'task_1', status: TaskStatus = 'active'): TaskProjection {
  return {
    task: {
      id,
      workspaceId: 'ws_1',
      idempotencyKey: `key_${id}`,
      objective: `Objective ${id}`,
      sessionKey: 'agent:ws:user/session',
      createdBy: 'user_1',
      status,
      currentRunId: 'run_1',
      stateVersion: 0,
      createdAt: '2026-10-27T10:00:00Z',
      updatedAt: '2026-10-27T10:00:00Z',
      endedAt: null,
    },
    runs: [],
    jobs: [],
    approvals: [],
    watches: [],
    evidence: [],
    associations: [],
    devices: [],
    summary: null,
    cursor: 0,
  };
}

function liveEvent(eventId: string, cursor = 3): RuntimeEvent {
  return {
    cursor,
    eventId,
    eventType: 'read_device',
    occurredAt: '2026-10-27T10:04:00Z',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: null,
    toolCallId: null,
    jobId: null,
    approvalId: null,
    deviceId: 'dev_live',
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: {},
  };
}

function taskStatusEvent(eventType: 'task.completed' | 'task.cancelled', cursor = 1): RuntimeEvent {
  return {
    cursor,
    eventId: `${eventType}:${cursor}`,
    eventType,
    occurredAt: '2026-10-27T10:04:00Z',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: null,
    toolCallId: null,
    jobId: null,
    approvalId: null,
    deviceId: null,
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: { stateVersion: 1 },
  };
}

function jobStateEvent(jobId: string, state: DeviceJobState, cursor = 1): RuntimeEvent {
  return {
    cursor,
    eventId: `job:${jobId}:${state}`,
    eventType: `job.${state}`,
    occurredAt: '2026-10-27T10:04:00Z',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: 'run_1',
    toolCallId: 'call_1',
    jobId,
    approvalId: null,
    deviceId: 'dev_1',
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: { state },
  };
}

function runningJob(): RuntimeJob {
  return {
    id: 'job_1',
    workspaceId: 'ws_1',
    deviceId: 'dev_1',
    tool: 'set_device',
    argsJson: { value: 21 },
    state: 'running',
    stopRequestedAt: null,
    progressJson: null,
    resultJson: null,
    taskId: 'task_1',
    runId: 'run_1',
    toolCallId: 'call_1',
    driverCallId: null,
    timeoutMs: 1000,
    startedAt: '2026-10-27T10:00:00Z',
    endedAt: null,
  };
}

function snapshotWithRunningJob(): TaskProjection {
  const value = snapshot();
  value.jobs = [runningJob()];
  value.associations = [{ deviceId: 'dev_1', sourceIds: [] }];
  return value;
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/');
  pause.mockReset();
  cancel.mockReset();
  close = vi.fn();
  connect.mockImplementation((input) => {
    streamCallbacks = input.callbacks;
    return () => close();
  });
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

async function mount(
  history: { event?: string; at?: string } | null = null,
  options: { view?: RuntimeView; device?: string | null } = {},
): Promise<void> {
  view = document.createElement('view-runtime-task') as RuntimeTaskView;
  view.taskId = 'task_1';
  view.view = options.view ?? 'loop';
  view.device = options.device ?? null;
  view.history = history;
  document.body.append(view);
  await view.updateComplete;
  await Promise.resolve();
  await view.updateComplete;
}

async function mountFromHash(hash: string): Promise<void> {
  const route = parseRuntimeHash(hash);
  if (!route || route.name !== 'task') throw new Error(`not a Task route: ${hash}`);
  window.history.replaceState({}, '', hash);
  view = document.createElement('view-runtime-task') as RuntimeTaskView;
  view.taskId = route.taskId;
  view.view = route.view;
  view.device = route.device;
  view.history = route.history;
  document.body.append(view);
  await view.updateComplete;
  await Promise.resolve();
  await view.updateComplete;
}

function executableControls(): HTMLElement[] {
  return [...view.querySelectorAll<HTMLElement>('[data-executable]')];
}

describe('Runtime Task detail shell', () => {
  it('renders the objective, status, navigation, IDs disclosure and projection-owned children', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();

    expect(view.querySelector('h1')?.textContent).toBe('Objective task_1');
    expect(view.querySelector('a[href="#/runtime"]')?.textContent).toContain('Runtime');
    expect(view.querySelector('a[href="#/runtime"] + a')?.textContent).toContain('Switch task');
    expect(view.textContent).toContain('active');
    expect(view.querySelector('summary')?.textContent).toContain('Details');
    expect(view.textContent).toContain('run_1');
    expect(view.querySelector('slot[name="task-actions"]')).not.toBeNull();
    expect(view.querySelector('runtime-loop')).not.toBeNull();
    expect(connect).toHaveBeenCalledOnce();
    expect(connect.mock.calls[0][0]).toMatchObject({
      workspaceId: 'ws_1',
      taskId: 'task_1',
      after: 0,
    });

    streamCallbacks.event({
      cursor: 1,
      eventId: 'event_1',
      eventType: 'read_device',
      occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1',
      taskId: 'task_1',
      runId: null,
      toolCallId: null,
      jobId: null,
      approvalId: null,
      deviceId: 'dev_1',
      sourceId: null,
      sourceSeq: null,
      evidenceId: null,
      data: {},
    } satisfies RuntimeEvent);
    await view.updateComplete;
    expect(view.querySelector('runtime-devices')?.textContent).toContain('dev_1');
  });

  it('closes SSE and destroys playback before replacing a switched Task projection', async () => {
    const order: string[] = [];
    close.mockImplementation(() => order.push('close'));
    get.mockImplementation((_ws, id) => {
      order.push(`get:${id}`);
      return Promise.resolve({ code: 0, msg: '', result: snapshot(id) });
    });
    await mount();
    view.descriptor = {
      protocol: 'mjpeg',
      url: 'https://localhost/frame',
      expiresAt: '2026-10-27T10:05:00Z',
    };
    await view.updateComplete;
    const playback = view.querySelector('runtime-playback') as RuntimePlayback;
    vi.spyOn(playback, 'destroy').mockImplementation(() => order.push('destroy'));

    view.taskId = 'task_2';
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(order).toEqual(['get:task_1', 'close', 'destroy', 'get:task_2']);
    expect(view.querySelector('h1')?.textContent).toBe('Objective task_2');
    expect(view.descriptor).toBeUndefined();
    expect(view.querySelector('runtime-playback')).toBeNull();
  });

  it('closes live SSE and freezes a fresh projection for same-Task history navigation', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    const liveCallbacks = streamCallbacks;

    view.history = { event: 'event_1' };
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(close).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledTimes(2);
    expect(connect).toHaveBeenCalledOnce();
    expect(view.textContent).toContain('History');
    liveCallbacks.event({
      cursor: 1,
      eventId: 'stale_live',
      eventType: 'read_device',
      occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1',
      taskId: 'task_1',
      runId: null,
      toolCallId: null,
      jobId: null,
      approvalId: null,
      deviceId: 'dev_stale',
      sourceId: null,
      sourceSeq: null,
      evidenceId: null,
      data: {},
    } satisfies RuntimeEvent);
    await view.updateComplete;
    expect(view.querySelector('runtime-loop')?.textContent).toContain('No agent activity yet');
    expect(view.querySelector('runtime-devices')?.textContent).not.toContain('dev_stale');
  });

  it('reloads a live projection and opens fresh SSE when leaving same-Task history', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount({ event: 'event_1' });
    expect(connect).not.toHaveBeenCalled();

    view.history = null;
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(get).toHaveBeenCalledTimes(2);
    expect(connect).toHaveBeenCalledOnce();
    expect(view.textContent).not.toContain('History');
  });

  it('freezes history without opening SSE and cleans up on disconnect', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount({ event: 'event_1' });
    expect(connect).not.toHaveBeenCalled();
    expect(view.textContent).toContain('History');

    view.descriptor = {
      protocol: 'mjpeg',
      url: 'https://localhost/frame',
      expiresAt: '2026-10-27T10:05:00Z',
    };
    await view.updateComplete;
    const playback = view.querySelector('runtime-playback') as RuntimePlayback;
    const destroy = vi.spyOn(playback, 'destroy');
    view.remove();
    expect(destroy).toHaveBeenCalledOnce();
  });

  it('keeps Devices mounted across same-level Loop and Flow tab navigation', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    const devices = view.querySelector('runtime-devices');
    expect(devices).not.toBeNull();
    expect(view.querySelector('runtime-loop')).not.toBeNull();

    view.querySelector<HTMLButtonElement>('button[data-view="flow"]')!.click();
    await view.updateComplete;

    expect(view.querySelector('runtime-devices')).toBe(devices);
    expect(view.querySelector('runtime-flow')).not.toBeNull();
    expect(view.querySelector('runtime-loop')).toBeNull();
    expect(window.location.hash).toBe('#/runtime/tasks/task_1?view=flow');
  });

  it('shares clamping, ARIA values, and persistence across seam keyboard input and collapse', async () => {
    vi.stubGlobal('innerWidth', 1200);
    localStorage.setItem('runtime.devices.width', '360');
    localStorage.setItem('runtime.devices.expanded', 'true');
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    const seam = view.querySelector<HTMLElement>('[role="separator"]')!;
    const key = async (key: string) => {
      seam.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
      await view.updateComplete;
    };

    expect(seam.getAttribute('aria-orientation')).toBe('vertical');
    expect(seam.getAttribute('aria-valuemin')).toBe('360');
    expect(seam.getAttribute('aria-valuemax')).toBe('900');
    await key('ArrowRight');
    expect(seam.getAttribute('aria-valuenow')).toBe('384');
    expect(localStorage.getItem('runtime.devices.width')).toBe('384');
    await key('ArrowLeft');
    await key('ArrowLeft');
    expect(seam.getAttribute('aria-valuenow')).toBe('360');
    for (let index = 0; index < 30; index += 1) await key('ArrowRight');
    expect(seam.getAttribute('aria-valuenow')).toBe('900');
    expect(localStorage.getItem('runtime.devices.width')).toBe('900');
    const input = document.createElement('input');
    view.append(input);
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
    await view.updateComplete;
    expect(localStorage.getItem('runtime.devices.width')).toBe('900');
    input.remove();

    view.querySelector<HTMLButtonElement>('.runtime-seam__toggle')!.click();
    await view.updateComplete;
    expect(localStorage.getItem('runtime.devices.expanded')).toBe('false');
    expect(view.querySelector('runtime-devices')).not.toBeNull();
    expect(view.querySelector<HTMLElement>('#runtime-devices-panel')!.style.width).toBe('36px');
  });

  it('clamps pointer drags and releases pointer capture on completion and cancellation', async () => {
    vi.stubGlobal('innerWidth', 1200);
    localStorage.setItem('runtime.devices.width', '400');
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    const seam = view.querySelector<HTMLElement>('[role="separator"]')!;
    const setCapture = vi.fn();
    const releaseCapture = vi.fn();
    seam.setPointerCapture = setCapture;
    seam.releasePointerCapture = releaseCapture;
    const pointer = (type: string, clientX: number) => {
      const event = new Event(type, { bubbles: true });
      Object.defineProperties(event, {
        pointerId: { value: 7 },
        clientX: { value: clientX },
        button: { value: 0 },
      });
      seam.dispatchEvent(event);
    };

    pointer('pointerdown', 400);
    pointer('pointermove', 350);
    pointer('pointerup', 350);
    expect(setCapture).toHaveBeenCalledWith(7);
    expect(releaseCapture).toHaveBeenCalledWith(7);
    expect(localStorage.getItem('runtime.devices.width')).toBe('450');

    pointer('pointerdown', 350);
    pointer('pointermove', 1000);
    pointer('pointercancel', 1000);
    expect(localStorage.getItem('runtime.devices.width')).toBe('360');
    expect(releaseCapture).toHaveBeenCalledTimes(2);
    pointer('pointermove', 0);
    expect(localStorage.getItem('runtime.devices.width')).toBe('360');
  });

  it('expands and focuses a located device without entering history', async () => {
    vi.stubGlobal(
      'matchMedia',
      vi.fn().mockReturnValue({
        matches: true,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }),
    );
    localStorage.setItem('runtime.devices.expanded', 'false');
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    streamCallbacks.event({
      cursor: 1,
      eventId: 'event_1',
      eventType: 'read_device',
      occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1',
      taskId: 'task_1',
      runId: null,
      toolCallId: null,
      jobId: null,
      approvalId: null,
      deviceId: 'dev_1',
      sourceId: null,
      sourceSeq: null,
      evidenceId: null,
      data: {},
    } satisfies RuntimeEvent);
    await view.updateComplete;
    const card = view.querySelector<HTMLElement>('[data-device-card][data-device-id="dev_1"]')!;
    const scroll = vi.fn();
    card.scrollIntoView = scroll;
    const focus = vi.spyOn(card.querySelector<HTMLElement>('h3')!, 'focus');

    view.querySelector<HTMLButtonElement>('runtime-loop button[data-device-id="dev_1"]')!.click();
    await view.updateComplete;

    expect(localStorage.getItem('runtime.devices.expanded')).toBe('true');
    expect(window.location.hash).toContain('device=dev_1');
    expect(window.location.hash).not.toContain('event=');
    expect(scroll).toHaveBeenCalledWith({ behavior: 'auto', block: 'nearest' });
    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  });

  it('sets event only for a selected Flow event', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    streamCallbacks.event({
      cursor: 1,
      eventId: 'event_1',
      eventType: 'read_device',
      occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1',
      taskId: 'task_1',
      runId: null,
      toolCallId: null,
      jobId: null,
      approvalId: null,
      deviceId: 'dev_1',
      sourceId: null,
      sourceSeq: null,
      evidenceId: null,
      data: {},
    } satisfies RuntimeEvent);
    await view.updateComplete;
    view.querySelector<HTMLButtonElement>('button[data-view="flow"]')!.click();
    await view.updateComplete;
    view.querySelector<HTMLButtonElement>('runtime-flow button[data-event-id="event_1"]')!.click();

    expect(window.location.hash).toContain('view=flow');
    expect(window.location.hash).toContain('event=event_1');
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;
    expect(connect).toHaveBeenCalledOnce();
  });

  it('refreshes through the existing API without opening a second stream', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    view
      .querySelector('runtime-loop')!
      .dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
    await Promise.resolve();
    await view.updateComplete;

    expect(get).toHaveBeenCalledTimes(2);
    expect(connect).toHaveBeenCalledOnce();
  });

  it('preserves an SSE event delivered while an older manual refresh snapshot is in flight', async () => {
    get.mockResolvedValueOnce({ code: 0, msg: '', result: snapshot() });
    let resolveRefresh!: (value: { code: number; msg: string; result: TaskProjection }) => void;
    get.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveRefresh = resolve;
        }),
    );
    await mount();
    view
      .querySelector('runtime-loop')!
      .dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
    await Promise.resolve();

    streamCallbacks.event({
      cursor: 101,
      eventId: 'event_during_refresh',
      eventType: 'read_device',
      occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1',
      taskId: 'task_1',
      runId: null,
      toolCallId: null,
      jobId: null,
      approvalId: null,
      deviceId: 'dev_live',
      sourceId: 'telemetry',
      sourceSeq: null,
      evidenceId: null,
      data: {},
    } satisfies RuntimeEvent);
    await view.updateComplete;
    resolveRefresh({ code: 0, msg: '', result: { ...snapshot(), cursor: 100 } });
    await Promise.resolve();
    await view.updateComplete;

    expect(
      view.querySelector('runtime-loop [data-event-id="event_during_refresh"]'),
    ).not.toBeNull();
    expect(view.querySelector('runtime-devices')?.textContent).toContain('dev_live');
    expect(connect).toHaveBeenCalledOnce();
    expect(get).toHaveBeenCalledTimes(2);
  });

  it('freezes history, removes executable controls, and resyncs back to live', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    const liveCallbacks = streamCallbacks;

    view.history = { event: 'event_2' };
    view.view = 'flow';
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    const banner = view.querySelector<HTMLElement>('[data-history-banner]');
    expect(banner?.textContent).toContain('History · read-only');
    expect(executableControls()).toEqual([]);
    expect(connect).toHaveBeenCalledOnce();

    liveCallbacks.event(liveEvent('event_3'));
    await view.updateComplete;
    const frozen = (view as unknown as { projection?: { eventIds: Set<string> } }).projection;
    expect(frozen?.eventIds.has('event_3')).toBe(false);

    const fetchesBefore = get.mock.calls.length;
    view.querySelector<HTMLButtonElement>('button[data-action="back-to-live"]')!.click();
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(window.location.hash).toBe('#/runtime/tasks/task_1?view=flow');
    expect(get.mock.calls.length - fetchesBefore).toBe(1);
    expect(view.querySelector('[data-history-banner]')).toBeNull();
    expect(connect).toHaveBeenCalledTimes(2);
  });

  it('shows a factual history time and preserves Task, view and device on Back to live', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mountFromHash(
      '#/runtime/tasks/task_1?view=flow&device=dev_1&at=2026-10-27T10%3A00%3A00Z',
    );

    expect(view.querySelector('[data-history-banner]')?.textContent).toContain(
      '2026-10-27T10:00:00Z',
    );
    view.querySelector<HTMLButtonElement>('button[data-action="back-to-live"]')!.click();
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(window.location.hash).toBe('#/runtime/tasks/task_1?view=flow&device=dev_1');
    expect(view.querySelector('[data-history-banner]')).toBeNull();
  });

  it('stays live when only a device is selected', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount(null, { device: 'dev_1' });

    expect(view.querySelector('[data-history-banner]')).toBeNull();
    expect(connect).toHaveBeenCalledOnce();
  });

  it('renders stale/offline as a separate connection banner, never as history or a terminal Task', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();

    streamCallbacks.connection('offline');
    await view.updateComplete;

    const connection = view.querySelector<HTMLElement>('[data-connection="offline"]');
    expect(connection).not.toBeNull();
    expect(connection?.textContent?.toLowerCase()).toContain('offline');
    expect(view.textContent).not.toContain('History · read-only');
    expect(view.querySelector('.runtime-task__status')?.textContent).toContain('active');
    expect(view.textContent).not.toContain('All stopped');
    expect(view.querySelector('[data-history-banner]')).toBeNull();
  });

  it('keeps history details and evidence readable while no mutation controls exist', async () => {
    const withEvidence = snapshot();
    withEvidence.associations = [{ deviceId: 'dev_1', sourceIds: ['camera'] }];
    withEvidence.evidence = [
      {
        evidenceId: 'evidence_history',
        mime: 'image/png',
        sourceSeq: 1,
        mediaTime: '2026-10-27T10:00:00Z',
        deviceId: 'dev_1',
        sourceId: 'camera',
        taskId: 'task_1',
        runId: null,
        toolCallId: null,
        jobId: null,
      },
    ];
    get.mockResolvedValue({ code: 0, msg: '', result: withEvidence });
    await mount({ event: 'event_2' });

    expect(view.querySelector('[data-evidence-id="evidence_history"]')).not.toBeNull();
    expect(executableControls()).toEqual([]);
    expect(view.querySelector('runtime-task-controls button[data-action="pause"]')).toBeNull();
    expect(view.querySelector('runtime-task-controls button[data-action="overflow"]')).toBeNull();
  });

  it('renders no executable controls for a terminal Task', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot('task_1', 'completed') });
    await mount();

    expect(executableControls()).toEqual([]);
    expect(view.querySelector('button[data-action="pause"]')).toBeNull();
  });

  it('installs the server projection returned by a header pause control', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    pause.mockResolvedValue({ code: 0, msg: '', result: snapshot('task_1', 'paused') });
    await mount();

    view
      .querySelector<HTMLButtonElement>('runtime-task-controls button[data-action="pause"]')!
      .click();
    await Promise.resolve();
    await Promise.resolve();
    await view.updateComplete;

    expect(pause).toHaveBeenCalledOnce();
    expect(view.querySelector('h1')?.textContent).toBe('Objective task_1');
    expect(view.querySelector('.runtime-task__status')?.textContent).toContain('paused');
    expect(view.querySelector('runtime-task-controls button[data-action="resume"]')).not.toBeNull();
    expect(connect).toHaveBeenCalledOnce();
  });

  it('removes Pause and Cancel when the live stream makes the Task terminal', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    expect(view.querySelector('runtime-task-controls button[data-action="pause"]')).not.toBeNull();
    expect(
      view.querySelector('runtime-task-controls button[data-action="overflow"]'),
    ).not.toBeNull();

    streamCallbacks.event(taskStatusEvent('task.completed'));
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    expect(view.querySelector('.runtime-task__status')?.textContent).toContain('completed');
    expect(view.querySelector('runtime-task-controls button[data-action="pause"]')).toBeNull();
    expect(view.querySelector('runtime-task-controls button[data-action="overflow"]')).toBeNull();
  });

  it('clears Cancelling when the live stream reports the Task cancelled', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    cancel.mockImplementation(() => new Promise<never>(() => {}));
    await mount();
    const controls = view.querySelector<RuntimeTaskControls>('runtime-task-controls')!;

    controls.querySelector<HTMLButtonElement>('button[data-action="overflow"]')!.click();
    await controls.updateComplete;
    controls.querySelector<HTMLButtonElement>('button[data-action="menu-cancel"]')!.click();
    await controls.updateComplete;
    controls.querySelector<HTMLButtonElement>('button[data-action="danger"]')!.click();
    await controls.updateComplete;

    expect(cancel).toHaveBeenCalledOnce();
    expect(controls.textContent).toContain('Cancelling');

    streamCallbacks.event(taskStatusEvent('task.cancelled'));
    await view.updateComplete;
    await controls.updateComplete;
    await Promise.resolve();
    await controls.updateComplete;

    expect(controls.textContent).not.toContain('Cancelling');
    expect(controls.querySelector('button[data-action="pause"]')).toBeNull();
    expect(controls.querySelector('button[data-action="overflow"]')).toBeNull();
  });

  it('shows Cancellation needs attention when cancelled_unknown arrives via the live stream', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithRunningJob() });
    await mount();
    const controls = view.querySelector<RuntimeTaskControls>('runtime-task-controls')!;
    expect(controls.querySelector('[data-attention="cancelled-unknown"]')).toBeNull();

    streamCallbacks.event(jobStateEvent('job_1', 'cancelled_unknown'));
    await view.updateComplete;
    await controls.updateComplete;

    expect(controls.querySelector('[data-attention="cancelled-unknown"]')).not.toBeNull();
    expect(controls.textContent).toContain('Cancellation needs attention');
    expect(controls.textContent).toContain('dev_1');
    expect(controls.textContent).not.toContain('All stopped');
  });
});
