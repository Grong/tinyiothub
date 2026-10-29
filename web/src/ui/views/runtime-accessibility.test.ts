import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { ApiError } from '../../api/client.js';
import { cancelRuntimeTask, getRuntimeTask } from '../../api/runtime.js';
import { connectRuntimeStream, type RuntimeStreamCallbacks } from '../../api/runtime-stream.js';
import type {
  DeviceJobState,
  RuntimeEvent,
  TaskProjection,
  TaskStatus,
} from '../../types/runtime.js';
import type { RuntimeTaskControls } from './runtime-task-controls.js';
import './runtime-task.js';
import type { RuntimeTaskView } from './runtime-task.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return {
    ...actual,
    getRuntimeTask: vi.fn(),
    pauseRuntimeTask: vi.fn(),
    resumeRuntimeTask: vi.fn(),
    cancelRuntimeTask: vi.fn(),
  };
});
vi.mock('../../api/runtime-stream.js', () => ({ connectRuntimeStream: vi.fn() }));

const get = vi.mocked(getRuntimeTask);
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
    summary: null,
    cursor: 0,
    associations: [{ deviceId: 'front-camera', sourceIds: ['live'] }],
    devices: [
      {
        deviceId: 'front-camera',
        health: 'ok',
        zone: 'Entrance',
        position: 'rack-1',
        sources: [{ id: 'live', dataKind: 'video' }],
        fields: [{ name: 'occupancy', value: '0', unit: null }],
      },
    ],
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
    deviceId: 'front-camera',
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: { state },
  };
}

function telemetryEvent(cursor = 2): RuntimeEvent {
  return {
    cursor,
    eventId: `telemetry:${cursor}`,
    eventType: 'read_device',
    occurredAt: '2026-10-27T10:05:00Z',
    workspaceId: 'ws_1',
    taskId: 'task_1',
    runId: 'run_1',
    toolCallId: null,
    jobId: null,
    approvalId: null,
    deviceId: 'front-camera',
    sourceId: null,
    sourceSeq: null,
    evidenceId: null,
    data: { occupancy: '0' },
  };
}

function taskStatusEvent(eventType: 'task.completed' | 'task.cancelled', cursor = 3): RuntimeEvent {
  return {
    cursor,
    eventId: `${eventType}:${cursor}`,
    eventType,
    occurredAt: '2026-10-27T10:06:00Z',
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

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (error: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await view.updateComplete;
  const controls = view.querySelector<RuntimeTaskControls>('runtime-task-controls');
  await controls?.updateComplete;
}

interface WorkbenchFixture {
  readonly view: RuntimeTaskView;
  readonly tabList: HTMLElement;
  readonly loopTab: HTMLButtonElement;
  readonly flowTab: HTMLButtonElement;
  readonly devicesTab: HTMLButtonElement;
  readonly seam: HTMLElement;
  readonly evidenceImage: HTMLImageElement;
  readonly connectionStatus: HTMLElement;
  readonly cancelTrigger: HTMLButtonElement;
  readonly deviceLink: HTMLButtonElement;
  readonly selectedDeviceHeading: HTMLElement;
  readonly announcer: HTMLElement;
  readonly dialogPrimary: HTMLButtonElement;
  readonly dialogDanger: HTMLButtonElement;
  readonly openCancel: () => Promise<void>;
  readonly escape: () => void;
}

function required<T extends Element>(selector: string): T {
  const element = view.querySelector<T>(selector);
  if (!element) throw new Error(`Missing element: ${selector}`);
  return element;
}

async function workbenchFixture(
  options: { offline?: boolean; mobile?: boolean } = {},
): Promise<WorkbenchFixture> {
  if (options.mobile) {
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: query === '(max-width: 599px)',
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }));
  }
  get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
  view = document.createElement('view-runtime-task') as RuntimeTaskView;
  view.taskId = 'task_1';
  view.view = 'loop';
  view.device = null;
  document.body.append(view);
  await view.updateComplete;
  await Promise.resolve();
  await view.updateComplete;

  if (options.offline !== false) {
    streamCallbacks.connection('offline');
    await view.updateComplete;
  }

  // The live image is the only <img> in the Runtime workbench; its alt is
  // derived from the selected/associated device and source metadata.
  view.descriptor = {
    protocol: 'mjpeg',
    url: 'https://camera.example/front.mjpg',
    expiresAt: '2026-10-27T10:05:00Z',
  };
  await view.updateComplete;
  await view.querySelector<HTMLElement & { updateComplete: Promise<unknown> }>('runtime-playback')
    ?.updateComplete;

  const controls = required<RuntimeTaskControls>('runtime-task-controls');
  return {
    view,
    get tabList() {
      return required<HTMLElement>('[role="tablist"]');
    },
    get loopTab() {
      return required<HTMLButtonElement>('[role="tab"][data-view="loop"]');
    },
    get flowTab() {
      return required<HTMLButtonElement>('[role="tab"][data-view="flow"]');
    },
    get devicesTab() {
      return required<HTMLButtonElement>('[role="tab"][data-view="devices"]');
    },
    get seam() {
      return required<HTMLElement>('[role="separator"]');
    },
    get evidenceImage() {
      const image =
        required<HTMLElement>('runtime-playback').shadowRoot?.querySelector<HTMLImageElement>(
          'img',
        );
      if (!image) throw new Error('Missing runtime-playback image');
      return image;
    },
    get connectionStatus() {
      return required<HTMLElement>('[data-connection="offline"]');
    },
    get cancelTrigger() {
      return required<HTMLButtonElement>('runtime-task-controls button[data-action="overflow"]');
    },
    get deviceLink() {
      return required<HTMLButtonElement>('runtime-devices button[data-device-id="front-camera"]');
    },
    get selectedDeviceHeading() {
      return required<HTMLElement>('[data-device-card][data-device-id="front-camera"] h3');
    },
    get announcer() {
      return required<HTMLElement>('[data-announcer]');
    },
    get dialogPrimary() {
      return required<HTMLButtonElement>('runtime-confirm-dialog button[data-action="primary"]');
    },
    get dialogDanger() {
      return required<HTMLButtonElement>('runtime-confirm-dialog button[data-action="danger"]');
    },
    openCancel: async () => {
      required<HTMLButtonElement>('runtime-task-controls button[data-action="overflow"]').click();
      await controls.updateComplete;
      required<HTMLButtonElement>(
        'runtime-task-controls button[data-action="menu-cancel"]',
      ).click();
      await controls.updateComplete;
      await view.updateComplete;
    },
    escape: () => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    },
  };
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/');
  get.mockReset();
  cancel.mockReset();
  close = vi.fn();
  connect.mockImplementation((input) => {
    streamCallbacks = input.callbacks;
    return () => close();
  });
  vi.stubGlobal('scrollIntoView', vi.fn());
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  view?.remove();
  localStorage.clear();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
});

describe('Runtime workbench accessibility contracts', () => {
  it('names tabs, separator, images, and status regions', async () => {
    const workbench = await workbenchFixture();

    expect(workbench.tabList.getAttribute('aria-label')).toBe('Runtime view');
    expect(workbench.seam.getAttribute('role')).toBe('separator');
    expect(workbench.seam.getAttribute('aria-valuemin')).toBe('360');
    expect(workbench.seam.getAttribute('aria-orientation')).toBe('vertical');
    expect(workbench.evidenceImage.alt).toContain('front');
    expect(workbench.evidenceImage.alt.toLowerCase()).not.toContain('people');
    expect(workbench.connectionStatus.getAttribute('role')).toBe('status');
  });

  it('returns focus after dialogs and supports device navigation by keyboard', async () => {
    const workbench = await workbenchFixture();

    workbench.cancelTrigger.focus();
    await workbench.openCancel();
    expect(document.activeElement).toBe(workbench.dialogPrimary);
    workbench.escape();
    await flush();
    expect(document.activeElement).toBe(workbench.cancelTrigger);

    workbench.deviceLink.focus();
    workbench.deviceLink.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
    );
    await flush();
    expect(document.activeElement).toBe(workbench.selectedDeviceHeading);
  });

  it('keeps Tab inside the dialog after focus escapes to the body', async () => {
    const workbench = await workbenchFixture();
    await workbench.openCancel();

    (document.activeElement as HTMLElement | null)?.blur();
    expect(document.activeElement).toBe(document.body);
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));

    expect(document.activeElement).toBe(workbench.dialogPrimary);
  });

  it('defers focus return to the overflow trigger until the cancel request settles', async () => {
    const pending = deferred<{ code: number; msg: string; result: TaskProjection }>();
    cancel.mockReturnValue(pending.promise);
    const workbench = await workbenchFixture();

    workbench.cancelTrigger.focus();
    await workbench.openCancel();
    workbench.dialogDanger.click();
    await flush();

    expect(cancel).toHaveBeenCalledOnce();
    expect(workbench.cancelTrigger.disabled).toBe(true);

    pending.reject(new ApiError(503, 'Cancel unavailable'));
    await flush();

    expect(workbench.cancelTrigger.disabled).toBe(false);
    expect(document.activeElement).toBe(workbench.cancelTrigger);
  });

  it('moves focus to the new objective only on Task switch, never on Details expansion', async () => {
    const workbench = await workbenchFixture();
    const summary = required<HTMLElement>('.runtime-task__details summary');
    summary.focus();
    summary.click();
    await flush();
    expect(document.activeElement).toBe(summary);

    get.mockResolvedValue({ code: 0, msg: '', result: snapshot('task_2') });
    view.taskId = 'task_2';
    await flush();
    await flush();

    expect(workbench.view.querySelector('h1')?.textContent).toBe('Objective task_2');
    expect(document.activeElement).toBe(workbench.view.querySelector('h1'));
  });

  it('announces only Task and Job transitions in one polite region', async () => {
    const workbench = await workbenchFixture({ offline: false });
    expect(workbench.announcer.getAttribute('aria-live')).toBe('polite');
    expect(workbench.announcer.textContent?.trim()).toBe('');
    expect(view.querySelectorAll('[data-announcer]')).toHaveLength(1);

    streamCallbacks.event(jobStateEvent('job_1', 'accepted'));
    await flush();
    expect(workbench.announcer.textContent).toContain('Job job_1');

    const afterJob = workbench.announcer.textContent;
    streamCallbacks.event(telemetryEvent());
    await flush();
    expect(workbench.announcer.textContent).toBe(afterJob);

    streamCallbacks.event(taskStatusEvent('task.completed'));
    await flush();
    expect(workbench.announcer.textContent).toContain('Task completed');
  });

  it('exposes same-level Loop, Flow and Devices tabs with keyboard activation', async () => {
    const workbench = await workbenchFixture({ mobile: true });

    expect(workbench.tabList.className).toContain('runtime-view-tabs');
    expect(workbench.devicesTab).not.toBeNull();
    expect(workbench.tabList.closest('.runtime-workbench__panel')).toBeNull();
    expect(workbench.loopTab.getAttribute('aria-selected')).toBe('true');
    expect(workbench.loopTab.tabIndex).toBe(0);
    expect(workbench.flowTab.tabIndex).toBe(-1);

    workbench.loopTab.focus();
    workbench.loopTab.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }),
    );
    await flush();
    expect(workbench.flowTab.getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(workbench.flowTab);

    workbench.flowTab.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }),
    );
    await flush();
    expect(workbench.devicesTab.getAttribute('aria-selected')).toBe('true');
    expect(required<HTMLElement>('.runtime-workbench').dataset.view).toBe('devices');
    expect(document.activeElement).toBe(workbench.devicesTab);

    workbench.devicesTab.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Home', bubbles: true }),
    );
    await flush();
    expect(workbench.loopTab.getAttribute('aria-selected')).toBe('true');
    expect(required<HTMLElement>('.runtime-workbench').dataset.view).toBe('loop');
  });

  it('keeps the Devices view out of the roving tabs above 599px', async () => {
    const workbench = await workbenchFixture();

    expect(view.querySelector('[role="tab"][data-view="devices"]')).toBeNull();
    expect(workbench.tabList.querySelectorAll('[role="tab"]')).toHaveLength(2);

    workbench.loopTab.focus();
    workbench.loopTab.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }),
    );
    await flush();
    expect(workbench.flowTab.getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(workbench.flowTab);

    workbench.flowTab.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }),
    );
    await flush();
    expect(workbench.loopTab.getAttribute('aria-selected')).toBe('true');
    expect(document.activeElement).toBe(workbench.loopTab);
    expect(required<HTMLElement>('.runtime-workbench').dataset.view).toBe('loop');
  });

  it('marks loading placeholders aria-hidden and surfaces load errors as alerts', async () => {
    const pending = deferred<{ code: number; msg: string; result: TaskProjection }>();
    get.mockReturnValue(pending.promise);
    view = document.createElement('view-runtime-task') as RuntimeTaskView;
    view.taskId = 'task_1';
    document.body.append(view);
    await view.updateComplete;
    await Promise.resolve();
    await view.updateComplete;

    const loading = required<HTMLElement>('[data-loading]');
    expect(loading.getAttribute('aria-hidden')).toBe('true');
    expect(required<HTMLElement>('.runtime-task').getAttribute('aria-busy')).toBe('true');

    pending.reject(new ApiError(503, 'Task unavailable'));
    await flush();

    expect(required<HTMLElement>('[role="alert"]').textContent).toContain('Task unavailable');
  });
});
