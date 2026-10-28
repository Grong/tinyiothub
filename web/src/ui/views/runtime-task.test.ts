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
    expect(view.querySelector('[data-projection="steps"]')).not.toBeNull();
    expect(connect).toHaveBeenCalledOnce();
    expect(connect.mock.calls[0][0]).toMatchObject({ workspaceId: 'ws_1', taskId: 'task_1', after: 0 });

    streamCallbacks.event({
      cursor: 1, eventId: 'event_1', eventType: 'read_device', occurredAt: '2026-10-27T10:01:00Z',
      workspaceId: 'ws_1', taskId: 'task_1', runId: null, toolCallId: null, jobId: null,
      approvalId: null, deviceId: 'dev_1', sourceId: null, sourceSeq: null, evidenceId: null, data: {},
    } satisfies RuntimeEvent);
    await view.updateComplete;
    expect(view.querySelector('[data-projection="devices"]')?.textContent).toContain('dev_1');
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
});
