import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { getPlaybackDescriptor } from '../../api/playback.js';
import {
  cancelRuntimeTask,
  getRuntimeTask,
  listRuntimeTaskEvents,
  pauseRuntimeTask,
} from '../../api/runtime.js';
import { connectRuntimeStream, type RuntimeStreamCallbacks } from '../../api/runtime-stream.js';
import type {
  DeviceJobState,
  PlaybackDescriptor,
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
    listRuntimeTaskEvents: vi.fn(),
    pauseRuntimeTask: vi.fn(),
    cancelRuntimeTask: vi.fn(),
  };
});
vi.mock('../../api/playback.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/playback.js')>();
  return { ...actual, getPlaybackDescriptor: vi.fn() };
});
vi.mock('../../api/runtime-stream.js', () => ({ connectRuntimeStream: vi.fn() }));

const get = vi.mocked(getRuntimeTask);
const events = vi.mocked(listRuntimeTaskEvents);
const pause = vi.mocked(pauseRuntimeTask);
const cancel = vi.mocked(cancelRuntimeTask);
const playback = vi.mocked(getPlaybackDescriptor);
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

function snapshotWithUnknownJob(): TaskProjection {
  const value = snapshotWithRunningJob();
  value.jobs = [{ ...runningJob(), state: 'cancelled_unknown' }];
  return value;
}

function snapshotWithCameras(): TaskProjection {
  const value = snapshot();
  value.associations = [
    { deviceId: 'cam-a', sourceIds: ['live'] },
    { deviceId: 'cam-b', sourceIds: ['live'] },
  ];
  value.devices = ['cam-a', 'cam-b'].map((deviceId) => ({
    deviceId,
    health: 'ok',
    zone: 'Entrance',
    position: 'rack-1',
    sources: [{ id: 'live', dataKind: 'video' }],
    fields: [],
  }));
  return value;
}

function snapshotWithCameraAndSensor(): TaskProjection {
  const value = snapshot();
  value.associations = [
    { deviceId: 'cam-a', sourceIds: ['live'] },
    { deviceId: 'sensor-1', sourceIds: [] },
  ];
  value.devices = [
    {
      deviceId: 'cam-a',
      health: 'ok',
      zone: 'Entrance',
      position: 'rack-1',
      sources: [{ id: 'live', dataKind: 'video' }],
      fields: [],
    },
    {
      deviceId: 'sensor-1',
      health: 'ok',
      zone: 'Entrance',
      position: 'rack-1',
      sources: [],
      fields: [{ name: 'temperature', value: '21', unit: 'C' }],
    },
  ];
  return value;
}

function playbackDescriptor(over: Partial<PlaybackDescriptor> = {}): PlaybackDescriptor {
  return {
    protocol: 'mjpeg',
    url: 'https://camera.example/frame.jpg',
    expiresAt: new Date(Date.now() + 60 * 60_000).toISOString(),
    ...over,
  };
}

function emptyTimeline() {
  return {
    code: 0,
    msg: '',
    result: { events: [] as RuntimeEvent[], oldestCursor: null, hasMore: false },
  };
}

function timelineWith(eventId = 'event_1', cursor = 1) {
  return {
    code: 0,
    msg: '',
    result: { events: [liveEvent(eventId, cursor)], oldestCursor: cursor, hasMore: false },
  };
}

async function settle(): Promise<void> {
  for (let index = 0; index < 4; index += 1) await Promise.resolve();
  await view.updateComplete;
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/');
  pause.mockReset();
  cancel.mockReset();
  events.mockReset();
  events.mockResolvedValue(emptyTimeline());
  playback.mockReset();
  close = vi.fn();
  connect.mockImplementation((input) => {
    streamCallbacks = input.callbacks;
    return () => close();
  });
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.useRealTimers();
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
  await settle();
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
  await settle();
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
    expect(view.querySelector('button[data-action="switch-task"]')?.textContent).toContain(
      'Switch task',
    );
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

  it('seeds existing timeline events before SSE and pages earlier with before=oldestCursor', async () => {
    const newest = liveEvent('event_10', 10);
    const older = liveEvent('event_5', 5);
    events.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: { events: [newest], oldestCursor: 10, hasMore: true },
    });
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();

    expect(events.mock.calls[0].slice(0, 4)).toEqual(['ws_1', 'task_1', 100, undefined]);
    expect(view.querySelector('runtime-loop [data-event-id="event_10"]')).not.toBeNull();
    expect(connect).toHaveBeenCalledOnce();
    expect(connect.mock.calls[0][0]).toMatchObject({ after: 10 });
    const more = view.querySelector<HTMLButtonElement>('[data-action="load-earlier"]');
    expect(more?.textContent).toContain('Load earlier events');

    events.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: { events: [older], oldestCursor: 5, hasMore: false },
    });
    more!.click();
    await settle();

    expect(events.mock.calls[1].slice(0, 4)).toEqual(['ws_1', 'task_1', 100, 10]);
    expect(view.querySelector('runtime-loop [data-event-id="event_5"]')).not.toBeNull();
    expect(view.querySelector('runtime-loop [data-event-id="event_10"]')).not.toBeNull();
    expect(view.querySelector('[data-action="load-earlier"]')).toBeNull();
  });

  it('shows an honest timeline loading error and retries without reopening SSE', async () => {
    events.mockRejectedValueOnce(new TypeError('offline'));
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();

    expect(view.querySelector('[data-timeline-error]')?.textContent).toContain(
      'Unable to load timeline',
    );
    expect(connect).toHaveBeenCalledOnce();

    events.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: { events: [liveEvent('event_4', 4)], oldestCursor: 4, hasMore: false },
    });
    view.querySelector<HTMLButtonElement>('[data-action="retry-timeline"]')!.click();
    await settle();

    expect(view.querySelector('runtime-loop [data-event-id="event_4"]')).not.toBeNull();
    expect(view.querySelector('[data-timeline-error]')).toBeNull();
    expect(connect).toHaveBeenCalledOnce();
  });

  it('shows an empty timeline state until seeded or streamed work exists', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    expect(view.querySelector('[data-timeline-empty]')?.textContent).toContain(
      'No timeline events yet',
    );

    streamCallbacks.event(liveEvent('event_2', 2));
    await view.updateComplete;
    expect(view.querySelector('[data-timeline-empty]')).toBeNull();
  });

  it('does not assert no agent activity while the initial timeline seed is loading', async () => {
    let release!: () => void;
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    events.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = () =>
            resolve({
              code: 0,
              msg: '',
              result: { events: [], oldestCursor: null, hasMore: false },
            });
        }),
    );
    await mount();

    expect(view.querySelector('[data-timeline-loading]')).not.toBeNull();
    expect(view.querySelector('runtime-loop')?.textContent).not.toContain('No agent activity yet');

    release();
    await settle();

    expect(view.querySelector('runtime-loop')?.textContent).toContain('No agent activity yet');
  });

  it('bounds a stalled initial timeline seed so SSE opens with an error and Retry', async () => {
    vi.useFakeTimers();
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    events.mockImplementation(() => new Promise(() => {}));
    await mount();

    expect(connect).not.toHaveBeenCalled();
    expect(view.querySelector('[data-timeline-loading]')).not.toBeNull();

    await vi.advanceTimersByTimeAsync(10_000);
    await settle();

    expect(connect).toHaveBeenCalledOnce();
    expect(view.querySelector('[data-timeline-error]')?.textContent).toContain(
      'Timeline request timed out',
    );
    expect(view.querySelector('[data-action="retry-timeline"]')).not.toBeNull();
  });

  it('keeps seeded timeline steps when a same-Task snapshot rewrites the projection', async () => {
    events.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: { events: [liveEvent('event_10', 10)], oldestCursor: 10, hasMore: false },
    });
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();

    streamCallbacks.snapshot(snapshot(), 20);
    await view.updateComplete;

    expect(view.querySelector('runtime-loop [data-event-id="event_10"]')).not.toBeNull();
  });

  it('destroys playback and clears the descriptor on a URL-driven device change', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithCameras() });
    playback.mockResolvedValue(playbackDescriptor());
    await mount();

    view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]')!.click();
    await settle();
    expect(view.querySelector('runtime-playback')).not.toBeNull();
    expect(view.descriptor).toBeDefined();

    view.device = 'cam-b';
    await view.updateComplete;

    expect(view.querySelector('runtime-playback')).toBeNull();
    expect(view.descriptor).toBeUndefined();

    view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]')!.click();
    await settle();
    const player = view.querySelector('runtime-playback') as RuntimePlayback;
    await player.updateComplete;
    const image = player.shadowRoot?.querySelector('img');
    expect(image?.alt).toContain('cam-b');
    expect(playback).toHaveBeenLastCalledWith('ws_1', 'cam-b', 'live', expect.any(AbortSignal));
  });

  it('never attributes a camera frame to a selected sensor-only device', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithCameraAndSensor() });
    playback.mockResolvedValue(playbackDescriptor());
    await mount();

    view.device = 'cam-a';
    await view.updateComplete;
    const play = view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]');
    expect(play).not.toBeNull();
    play!.click();
    await settle();
    const player = view.querySelector('runtime-playback') as RuntimePlayback;
    await player.updateComplete;
    const image = player.shadowRoot?.querySelector('img');
    expect(image?.alt).toContain('cam-a');
    expect(image?.alt).not.toContain('sensor-1');
    expect(playback).toHaveBeenLastCalledWith('ws_1', 'cam-a', 'live', expect.any(AbortSignal));

    view.device = 'sensor-1';
    await view.updateComplete;

    expect(view.querySelector('runtime-playback')).toBeNull();
    expect(view.querySelector('button[data-action="play-evidence"]')).toBeNull();
    expect(view.descriptor).toBeUndefined();
  });

  it('fails closed on a cold history URL instead of showing live facts under a past banner', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mountFromHash('#/runtime/tasks/task_1?at=2026-10-27T10%3A00%3A00Z');

    expect(view.querySelector('[data-history-banner]')).toBeNull();
    expect(view.querySelector('[data-history-notice]')?.textContent).toContain(
      'History is no longer available from this link',
    );
    expect(connect).toHaveBeenCalledOnce();
    expect(events).toHaveBeenCalledOnce();
    expect(window.location.hash).toBe('#/runtime/tasks/task_1');
    expect(view.querySelector('[data-timeline-loading]')).toBeNull();
    expect(view.querySelector('[data-timeline-empty]')).not.toBeNull();
  });

  it('fails closed when same-Task history navigation races the initial snapshot', async () => {
    const pending: Array<(value: { code: number; msg: string; result: TaskProjection }) => void> =
      [];
    get.mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
    await mount();
    expect(pending).toHaveLength(1);

    view.history = { at: '2026-10-27T10:00:00Z' };
    await view.updateComplete;
    expect(get).toHaveBeenCalledTimes(2);

    for (const resolve of pending) resolve({ code: 0, msg: '', result: snapshot() });
    await settle();

    expect(view.querySelector('[data-history-banner]')).toBeNull();
    expect(view.querySelector('[data-history-notice]')?.textContent).toContain(
      'History is no longer available from this link',
    );
    expect(window.location.hash).toBe('#/runtime/tasks/task_1');
    expect(connect).toHaveBeenCalledOnce();
    expect(events).toHaveBeenCalledOnce();
  });

  it('fails closed when a stale event selection survives a router history exit', async () => {
    events.mockResolvedValueOnce(timelineWith());
    const pending: Array<(value: { code: number; msg: string; result: TaskProjection }) => void> =
      [];
    get.mockResolvedValueOnce({ code: 0, msg: '', result: snapshot() });
    get.mockResolvedValueOnce({ code: 0, msg: '', result: snapshot() });
    await mount(null, { view: 'flow' });

    view.querySelector<HTMLButtonElement>('[data-event-id="event_1"]')!.click();
    await settle();
    expect(view.querySelector('[data-history-banner]')).not.toBeNull();
    expect(get).toHaveBeenCalledTimes(2);

    get.mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
    view.history = null;
    await settle();
    expect(pending).toHaveLength(1);
    expect(view.querySelector('[data-history-banner]')).toBeNull();

    view.history = { at: '2026-10-27T10:00:00Z' };
    await view.updateComplete;

    for (const resolve of pending) resolve({ code: 0, msg: '', result: snapshot() });
    await settle();

    expect(view.querySelector('[data-history-banner]')).toBeNull();
    expect(view.querySelector('[data-history-notice]')?.textContent).toContain(
      'History is no longer available from this link',
    );
    expect(window.location.hash).toBe('#/runtime/tasks/task_1?view=flow');
    expect(connect).toHaveBeenCalledTimes(2);
  });

  it('tears down playback when live device facts remove the playing source', async () => {
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithCameras() });
    playback.mockResolvedValue(playbackDescriptor());
    await mount();
    view.device = 'cam-a';
    await view.updateComplete;
    view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]')!.click();
    await settle();
    const player = view.querySelector('runtime-playback') as RuntimePlayback;
    await player.updateComplete;
    expect(player.shadowRoot?.querySelector('img')?.alt).toContain('cam-a');

    streamCallbacks.event({
      ...liveEvent('event_cam_facts', 4),
      deviceId: 'cam-a',
      data: { deviceFacts: { deviceId: 'cam-a', health: 'ok', sources: [] } },
    });
    await settle();

    expect(view.querySelector('runtime-playback')).toBeNull();
    expect(view.descriptor).toBeUndefined();
  });

  it('discards a descriptor that resolves after the playback target changed', async () => {
    let resolveDescriptor!: (descriptor: PlaybackDescriptor) => void;
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithCameras() });
    playback.mockImplementation(
      () => new Promise<PlaybackDescriptor>((resolve) => (resolveDescriptor = resolve)),
    );
    await mount();
    view.device = 'cam-a';
    await view.updateComplete;
    view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]')!.click();
    await view.updateComplete;
    expect(playback).toHaveBeenCalledOnce();

    streamCallbacks.event({
      ...liveEvent('event_cam_facts', 4),
      deviceId: 'cam-a',
      data: {
        deviceFacts: { deviceId: 'cam-a', sources: [{ id: 'live-2', dataKind: 'video' }] },
      },
    });
    await settle();

    resolveDescriptor(playbackDescriptor());
    await settle();

    expect(view.descriptor).toBeUndefined();
    expect(view.querySelector('runtime-playback')).toBeNull();
    expect(view.querySelector('button[data-action="play-evidence"]')).not.toBeNull();
  });

  it('keeps the rendered camera alt when load-earlier facts remove its source', async () => {
    events.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: { events: [liveEvent('event_seeded', 5)], oldestCursor: 5, hasMore: true },
    });
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithCameras() });
    playback.mockResolvedValue(playbackDescriptor());
    await mount();
    view.device = 'cam-a';
    await view.updateComplete;
    view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]')!.click();
    await settle();

    events.mockResolvedValueOnce({
      code: 0,
      msg: '',
      result: {
        events: [
          {
            ...liveEvent('event_old_facts', 1),
            deviceId: 'cam-a',
            data: { deviceFacts: { deviceId: 'cam-a', sources: [] } },
          },
        ],
        oldestCursor: 1,
        hasMore: false,
      },
    });
    view.querySelector<HTMLButtonElement>('button[data-action="load-earlier"]')!.click();
    await settle();

    const player = view.querySelector('runtime-playback') as RuntimePlayback;
    expect(player).not.toBeNull();
    await player.updateComplete;
    const image = player.shadowRoot?.querySelector('img');
    expect(image?.alt).toContain('cam-a');
    expect(image?.alt).toContain('live');
    // The resolver now disagrees (cam-a has no video source): the surviving alt
    // must come from the stored descriptor target, not a fresh resolution.
    expect(view.querySelector('button[data-action="play-evidence"]')).toBeNull();
  });

  it('refreshes a descriptor before expiry and cancels the timer when playback stops', async () => {
    vi.useFakeTimers();
    const expiresAt = new Date(Date.now() + 60_000).toISOString();
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithCameras() });
    playback.mockResolvedValue(playbackDescriptor({ expiresAt }));
    await mount();

    view.querySelector<HTMLButtonElement>('button[data-action="play-evidence"]')!.click();
    await settle();
    expect(playback).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(30_000);
    expect(playback).toHaveBeenCalledTimes(2);

    view.device = 'cam-b';
    await view.updateComplete;
    await vi.advanceTimersByTimeAsync(120_000);
    expect(playback).toHaveBeenCalledTimes(2);
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
      expiresAt: '2099-10-27T10:05:00Z',
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
    events.mockResolvedValueOnce(timelineWith());
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
    expect(view.querySelector('[data-history-notice]')).toBeNull();
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
    expect(view.querySelector('runtime-loop [data-event-id="event_1"]')).not.toBeNull();
    expect(view.querySelector('runtime-loop')?.textContent).not.toContain('stale_live');
    expect(view.querySelector('runtime-devices')?.textContent).not.toContain('dev_stale');
  });

  it('reloads a live projection and opens fresh SSE when leaving same-Task history', async () => {
    events.mockResolvedValueOnce(timelineWith());
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    expect(connect).toHaveBeenCalledOnce();

    view.history = { event: 'event_1' };
    await settle();
    expect(view.textContent).toContain('History');

    view.history = null;
    await settle();

    expect(get).toHaveBeenCalledTimes(3);
    expect(connect).toHaveBeenCalledTimes(2);
    expect(view.textContent).not.toContain('History');
  });

  it('freezes in-page history without reopening SSE and cleans up on disconnect', async () => {
    events.mockResolvedValueOnce(timelineWith());
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    expect(connect).toHaveBeenCalledOnce();

    view.history = { event: 'event_1' };
    await settle();
    expect(view.textContent).toContain('History');
    expect(connect).toHaveBeenCalledOnce();

    view.descriptor = {
      protocol: 'mjpeg',
      url: 'https://localhost/frame',
      expiresAt: '2099-10-27T10:05:00Z',
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
    await key('ArrowLeft');
    expect(seam.getAttribute('aria-valuenow')).toBe('384');
    expect(localStorage.getItem('runtime.devices.width')).toBe('384');
    await key('ArrowRight');
    await key('ArrowRight');
    expect(seam.getAttribute('aria-valuenow')).toBe('360');
    for (let index = 0; index < 30; index += 1) await key('ArrowLeft');
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
    expect(view.querySelector<HTMLElement>('#runtime-devices-panel')!.style.width).toBe('44px');
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
    events.mockResolvedValueOnce(timelineWith());
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();

    view.history = { event: 'event_2' };
    view.view = 'flow';
    await settle();

    const banner = view.querySelector<HTMLElement>('[data-history-banner]');
    expect(banner?.textContent).toContain('History · read-only');
    expect(executableControls()).toEqual([]);
    expect(connect).toHaveBeenCalledOnce();
    expect(view.querySelector('[data-event-id="event_1"]')).not.toBeNull();

    const fetchesBefore = get.mock.calls.length;
    view.querySelector<HTMLButtonElement>('button[data-action="back-to-live"]')!.click();
    await settle();

    expect(window.location.hash).toBe('#/runtime/tasks/task_1?view=flow');
    expect(get.mock.calls.length - fetchesBefore).toBe(1);
    expect(view.querySelector('[data-history-banner]')).toBeNull();
    expect(connect).toHaveBeenCalledTimes(2);
  });

  it('suppresses refresh status in read-only history and keeps the frozen base', async () => {
    events.mockResolvedValueOnce(timelineWith());
    get.mockResolvedValue({ code: 0, msg: '', result: snapshotWithUnknownJob() });
    await mount();

    expect(view.querySelectorAll('[data-action="refresh-status"]').length).toBeGreaterThan(0);

    view.history = { at: '2026-10-27T10:00:00Z' };
    await settle();

    expect(view.querySelector('[data-history-banner]')).not.toBeNull();
    expect(view.querySelectorAll('[data-action="refresh-status"]')).toHaveLength(0);

    const fetchesBefore = get.mock.calls.length;
    view
      .querySelector('runtime-task-controls')!
      .dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
    await settle();
    expect(get.mock.calls.length).toBe(fetchesBefore);
  });

  it('shows a factual history time and preserves Task, view and device on Back to live', async () => {
    events.mockResolvedValueOnce(timelineWith());
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mountFromHash('#/runtime/tasks/task_1?view=flow&device=dev_1');
    view.history = { at: '2026-10-27T10:00:00Z' };
    await settle();

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
    expect(view.querySelector('.runtime-task__status')?.textContent).toContain('Active');
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
        expiresAt: '2099-10-27T10:00:00Z',
        deviceId: 'dev_1',
        sourceId: 'camera',
        taskId: 'task_1',
        runId: null,
        toolCallId: null,
        jobId: null,
      },
    ];
    events.mockResolvedValueOnce(timelineWith());
    get.mockResolvedValue({ code: 0, msg: '', result: withEvidence });
    await mount();
    view.history = { event: 'event_2' };
    await settle();

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
    expect(view.querySelector('.runtime-task__status')?.textContent).toContain('Paused');
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

    expect(view.querySelector('.runtime-task__status')?.textContent).toContain('Completed');
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
