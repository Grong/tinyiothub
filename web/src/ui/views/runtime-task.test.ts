import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { getRuntimeTask } from '../../api/runtime.js';
import { connectRuntimeStream, type RuntimeStreamCallbacks } from '../../api/runtime-stream.js';
import type { RuntimeEvent, TaskProjection } from '../../types/runtime.js';
import type { RuntimePlayback } from './runtime-playback.js';
import './runtime-task.js';
import type { RuntimeTaskView } from './runtime-task.js';

vi.mock('../../api/runtime.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../api/runtime.js')>();
  return { ...actual, getRuntimeTask: vi.fn() };
});
vi.mock('../../api/runtime-stream.js', () => ({ connectRuntimeStream: vi.fn() }));

const get = vi.mocked(getRuntimeTask);
const connect = vi.mocked(connectRuntimeStream);
let view: RuntimeTaskView;
let close: Mock<() => void>;
let streamCallbacks: RuntimeStreamCallbacks;

function snapshot(id = 'task_1'): TaskProjection {
  return {
    task: {
      id, workspaceId: 'ws_1', idempotencyKey: `key_${id}`, objective: `Objective ${id}`,
      sessionKey: 'agent:ws:user/session', createdBy: 'user_1', status: 'active', currentRunId: 'run_1',
      stateVersion: 0, createdAt: '2026-10-27T10:00:00Z', updatedAt: '2026-10-27T10:00:00Z', endedAt: null,
    },
    runs: [], jobs: [], approvals: [], watches: [], evidence: [], summary: null, cursor: 0,
  };
}

beforeEach(() => {
  localStorage.setItem('workspace-id', 'ws_1');
  window.history.replaceState({}, '', '/');
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

async function mount(history: { event?: string; at?: string } | null = null): Promise<void> {
  view = document.createElement('view-runtime-task') as RuntimeTaskView;
  view.taskId = 'task_1';
  view.history = history;
  document.body.append(view);
  await view.updateComplete;
  await Promise.resolve();
  await view.updateComplete;
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
    expect(connect.mock.calls[0][0]).toMatchObject({ workspaceId: 'ws_1', taskId: 'task_1', after: 0 });

    streamCallbacks.event({
      cursor: 1, eventId: 'event_1', eventType: 'read_device', occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1', taskId: 'task_1', runId: null, toolCallId: null, jobId: null,
      approvalId: null, deviceId: 'dev_1', sourceId: null, sourceSeq: null, evidenceId: null, data: {},
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
    view.descriptor = { protocol: 'mjpeg', url: 'https://localhost/frame', expiresAt: '2026-10-27T10:05:00Z' };
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
      cursor: 1, eventId: 'stale_live', eventType: 'read_device', occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1', taskId: 'task_1', runId: null, toolCallId: null, jobId: null,
      approvalId: null, deviceId: 'dev_stale', sourceId: null, sourceSeq: null, evidenceId: null, data: {},
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

    view.descriptor = { protocol: 'mjpeg', url: 'https://localhost/frame', expiresAt: '2026-10-27T10:05:00Z' };
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

    view.querySelector<HTMLButtonElement>('button[aria-controls="runtime-devices-panel"]')!.click();
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
      Object.defineProperties(event, { pointerId: { value: 7 }, clientX: { value: clientX }, button: { value: 0 } });
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
    vi.stubGlobal('matchMedia', vi.fn().mockReturnValue({
      matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn(),
    }));
    localStorage.setItem('runtime.devices.expanded', 'false');
    get.mockResolvedValue({ code: 0, msg: '', result: snapshot() });
    await mount();
    streamCallbacks.event({
      cursor: 1, eventId: 'event_1', eventType: 'read_device', occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1', taskId: 'task_1', runId: null, toolCallId: null, jobId: null,
      approvalId: null, deviceId: 'dev_1', sourceId: null, sourceSeq: null, evidenceId: null, data: {},
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
      cursor: 1, eventId: 'event_1', eventType: 'read_device', occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1', taskId: 'task_1', runId: null, toolCallId: null, jobId: null,
      approvalId: null, deviceId: 'dev_1', sourceId: null, sourceSeq: null, evidenceId: null, data: {},
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
    view.querySelector('runtime-loop')!.dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
    await Promise.resolve();
    await view.updateComplete;

    expect(get).toHaveBeenCalledTimes(2);
    expect(connect).toHaveBeenCalledOnce();
  });
});
