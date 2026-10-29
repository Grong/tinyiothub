import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { connectRuntimeStream, type RuntimeStreamCallbacks } from './runtime-stream.js';
import type { RuntimeEvent, RuntimeTask, TaskProjection } from '../types/runtime.js';

function task(over: Partial<RuntimeTask> = {}): RuntimeTask {
  return {
    id: 'task_1',
    workspaceId: 'ws_1',
    idempotencyKey: 'key_1',
    objective: 'Inspect pumps',
    sessionKey: null,
    createdBy: 'user_1',
    status: 'active',
    currentRunId: null,
    stateVersion: 0,
    createdAt: '2026-10-27T10:00:00Z',
    updatedAt: '2026-10-27T10:00:00Z',
    endedAt: null,
    ...over,
  };
}

function snapshotWithNoDevices(over: Partial<TaskProjection> = {}): TaskProjection {
  return {
    task: task(),
    runs: [],
    jobs: [],
    approvals: [],
    watches: [],
    evidence: [],
    associations: [],
    devices: [],
    summary: null,
    cursor: 0,
    ...over,
  };
}

function runtimeEvent(
  eventId: string,
  eventType: string,
  data: Record<string, unknown> = {},
  over: Partial<RuntimeEvent> = {},
): RuntimeEvent {
  return {
    cursor: Number(eventId.replace(/\D/g, '')) || 1,
    eventId,
    eventType,
    occurredAt: '2026-10-27T10:01:00Z',
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
    data,
    ...over,
  };
}

function frame(name: string, data: unknown): string {
  return `event: ${name}\ndata: ${JSON.stringify(data)}\n\n`;
}

function controlledResponse() {
  const encoder = new TextEncoder();
  let controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  let cancelled = false;
  const body = new ReadableStream<Uint8Array>({
    start(value) {
      controller = value;
    },
    cancel() {
      cancelled = true;
    },
  });
  const readerCancel = vi.fn();
  const getReader = body.getReader.bind(body);
  Object.defineProperty(body, 'getReader', {
    value: () => {
      const reader = getReader();
      const cancel = reader.cancel.bind(reader);
      reader.cancel = (reason?: unknown) => {
        readerCancel();
        return cancel(reason);
      };
      return reader;
    },
  });
  return {
    response: new Response(body, { status: 200 }),
    readerCancel,
    push(text: string) {
      controller?.enqueue(encoder.encode(text));
    },
    error() {
      controller?.error(new Error('aborted'));
    },
    get cancelled() {
      return cancelled;
    },
  };
}

function streamingFetch() {
  const streams: ReturnType<typeof controlledResponse>[] = [];
  const mock = vi.fn((_url: string, init?: RequestInit) => {
    const stream = controlledResponse();
    streams.push(stream);
    init?.signal?.addEventListener('abort', () => stream.error());
    return Promise.resolve(stream.response);
  });
  return { mock, streams };
}

function callbacks(): RuntimeStreamCallbacks & { calls: string[] } {
  const calls: string[] = [];
  return {
    calls,
    snapshot: (_projection, cursor) => calls.push(`snapshot:${cursor}`),
    event: (event) => calls.push(`event:${event.eventId}`),
    resync: (_projection, cursor) => calls.push(`resync:${cursor}`),
    connection: (state) => calls.push(`connection:${state}`),
  };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
  sessionStorage.setItem('auth-token', 'secret-token');
  localStorage.setItem('workspace-id', 'header-workspace');
});

afterEach(() => {
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('Runtime SSE cursor client', () => {
  it('sends auth/workspace headers and handles split snapshot, event and resync frames', async () => {
    const { mock, streams } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    const handler = callbacks();
    const close = connectRuntimeStream({
      workspaceId: 'ws/1',
      taskId: 'task:1',
      after: 3,
      callbacks: handler,
    });
    await flush();

    const [url, init] = mock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/workspaces/ws%2F1/agent/tasks/task%3A1/stream?after=3');
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer secret-token');
    expect(new Headers(init.headers).get('X-Workspace-Id')).toBe('header-workspace');

    const snapshot = snapshotWithNoDevices({ task: task({ id: 'task:1' }), cursor: 4 });
    streams[0].push(
      `event: snapshot\ndata: ${JSON.stringify({ snapshot, cursor: 4 }).slice(0, 40)}`,
    );
    await flush();
    expect(handler.calls.filter((call) => call.startsWith('snapshot'))).toEqual([]);
    // Send a complete frame instead: the partial frame above intentionally remains buffered.
    const remainder = JSON.stringify({ snapshot, cursor: 4 }).slice(40);
    streams[0].push(`${remainder}\n\n`);
    const first = runtimeEvent('event_5', 'read_device', {}, { cursor: 5, taskId: 'task:1' });
    streams[0].push(frame('event', { event: first }) + frame('event', { event: first }));
    streams[0].push(
      frame('resync', {
        snapshot: snapshotWithNoDevices({ task: task({ id: 'task:1' }), cursor: 8 }),
        cursor: 8,
        reason: 'lagged',
      }),
    );
    await flush();

    expect(handler.calls).toEqual([
      'connection:connecting',
      'connection:online',
      'snapshot:4',
      'event:event_5',
      'resync:8',
    ]);
    close();
    await flush();
    expect((mock.mock.calls[0] as unknown as [string, RequestInit])[1].signal?.aborted).toBe(true);
    expect(streams[0].readerCancel).toHaveBeenCalledOnce();
  });

  it('reconnects from the last cursor with bounded exponential backoff', async () => {
    vi.useFakeTimers();
    const first = controlledResponse();
    const second = controlledResponse();
    const mock = vi
      .fn()
      .mockImplementationOnce(() => Promise.resolve(first.response))
      .mockImplementation(() => Promise.resolve(second.response));
    vi.stubGlobal('fetch', mock);
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      callbacks: callbacks(),
      baseDelayMs: 1000,
      maxDelayMs: 2000,
      jitterMs: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    first.push(
      frame('event', { event: runtimeEvent('event_7', 'task.paused', {}, { cursor: 7 }) }),
    );
    await vi.advanceTimersByTimeAsync(0);
    first.error();
    await vi.advanceTimersByTimeAsync(999);
    expect(mock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(mock).toHaveBeenCalledTimes(2);
    expect(mock.mock.calls[1][0]).toContain('after=7');
    close();
  });

  it('drops stale-generation responses and events after a Task switch', async () => {
    const { mock, streams } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    let current = true;
    const handler = callbacks();
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'old',
      callbacks: handler,
      isCurrent: () => current,
    });
    await flush();
    current = false;
    streams[0].push(frame('snapshot', { snapshot: snapshotWithNoDevices(), cursor: 0 }));
    streams[0].push(frame('event', { event: runtimeEvent('event_1', 'read_device') }));
    await flush();

    expect(handler.calls).toEqual(['connection:connecting', 'connection:online']);
    close();
    expect(mock).toHaveBeenCalledTimes(1);
  });

  it('drops frames for another Task without logging credentials or objectives', async () => {
    const { mock, streams } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    const handler = callbacks();
    const close = connectRuntimeStream({ workspaceId: 'ws', taskId: 'task_1', callbacks: handler });
    await flush();
    streams[0].push(
      frame('snapshot', {
        snapshot: snapshotWithNoDevices({
          task: { ...snapshotWithNoDevices().task, id: 'other', objective: 'private objective' },
        }),
        cursor: 0,
      }),
    );
    streams[0].push(
      frame('event', {
        event: runtimeEvent(
          'event_1',
          'read_device',
          { text: 'private objective' },
          { taskId: 'other' },
        ),
      }),
    );
    await flush();

    expect(handler.calls).toEqual(['connection:connecting', 'connection:online']);
    expect(log).not.toHaveBeenCalled();
    close();
  });

  it('delivers replay events overlapping the snapshot without acknowledging them early', async () => {
    const { mock, streams } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    const handler = callbacks();
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      after: 10,
      callbacks: handler,
    });
    await flush();
    streams[0].push(
      frame('snapshot', {
        snapshot: snapshotWithNoDevices({ cursor: 20 }),
        cursor: 20,
      }),
    );
    streams[0].push(
      frame('event', { event: runtimeEvent('event_11', 'read_device', {}, { cursor: 11 }) }),
    );
    streams[0].push(
      frame('event', { event: runtimeEvent('event_12', 'read_device', {}, { cursor: 12 }) }),
    );
    await flush();

    expect(handler.calls).toEqual([
      'connection:connecting',
      'connection:online',
      'snapshot:20',
      'event:event_11',
      'event:event_12',
    ]);
    close();
  });

  it('reconnects from the last delivered replay event after a partial replay', async () => {
    vi.useFakeTimers();
    const first = controlledResponse();
    const second = controlledResponse();
    const mock = vi
      .fn()
      .mockImplementationOnce(() => Promise.resolve(first.response))
      .mockImplementation(() => Promise.resolve(second.response));
    vi.stubGlobal('fetch', mock);
    const handler = callbacks();
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      after: 10,
      callbacks: handler,
      baseDelayMs: 1000,
      jitterMs: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    first.push(
      frame('snapshot', {
        snapshot: snapshotWithNoDevices({ cursor: 20 }),
        cursor: 20,
      }),
    );
    first.push(
      frame('event', { event: runtimeEvent('event_11', 'read_device', {}, { cursor: 11 }) }),
    );
    await vi.advanceTimersByTimeAsync(0);
    first.error();
    await vi.advanceTimersByTimeAsync(1000);

    expect(mock.mock.calls[1][0]).toContain('after=11');
    second.push(
      frame('snapshot', {
        snapshot: snapshotWithNoDevices({ cursor: 20 }),
        cursor: 20,
      }),
    );
    for (let cursor = 12; cursor <= 20; cursor += 1) {
      second.push(
        frame('event', {
          event: runtimeEvent(`event_${cursor}`, 'read_device', {}, { cursor }),
        }),
      );
    }
    await vi.advanceTimersByTimeAsync(0);

    const delivered = handler.calls.filter((call) => call.startsWith('event:'));
    expect(delivered).toEqual(
      Array.from({ length: 10 }, (_, index) => `event:event_${index + 11}`),
    );
    close();
  });

  it('cancels a non-OK response body before reconnecting', async () => {
    vi.useFakeTimers();
    const response = controlledResponse();
    const nonOk = new Response(response.response.body, { status: 500 });
    const mock = vi.fn(() => Promise.resolve(nonOk));
    vi.stubGlobal('fetch', mock);
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      callbacks: callbacks(),
      jitterMs: 0,
    });
    await vi.advanceTimersByTimeAsync(0);

    expect(response.cancelled).toBe(true);
    close();
  });

  it('stops permanently on 401 instead of retrying with stale credentials', async () => {
    vi.useFakeTimers();
    const unauthorized = new Response('unauthorized', { status: 401 });
    const mock = vi.fn(() => Promise.resolve(unauthorized));
    vi.stubGlobal('fetch', mock);
    const handler = callbacks();
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      callbacks: handler,
      jitterMs: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(120_000);

    expect(mock).toHaveBeenCalledTimes(1);
    expect(handler.calls).toContain('connection:offline');
    close();
  });

  it('keeps escalating backoff when a stream opens but never stays healthy', async () => {
    vi.useFakeTimers();
    const first = controlledResponse();
    const second = controlledResponse();
    const third = controlledResponse();
    const mock = vi
      .fn()
      .mockImplementationOnce(() => Promise.resolve(first.response))
      .mockImplementationOnce(() => Promise.resolve(second.response))
      .mockImplementation(() => Promise.resolve(third.response));
    vi.stubGlobal('fetch', mock);
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      callbacks: callbacks(),
      baseDelayMs: 1000,
      maxDelayMs: 30_000,
      jitterMs: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    first.error();
    await vi.advanceTimersByTimeAsync(1000);
    expect(mock).toHaveBeenCalledTimes(2);
    second.error();
    await vi.advanceTimersByTimeAsync(1_999);
    expect(mock).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(mock).toHaveBeenCalledTimes(3);
    close();
  });

  it('bounds replay dedupe so a long-lived stream cannot grow memory forever', async () => {
    const { mock, streams } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    const handler = callbacks();
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task_1',
      callbacks: handler,
      maxDedupeIds: 2,
    });
    await flush();
    for (const cursor of [1, 2, 3, 1]) {
      streams[0].push(
        frame('event', {
          event: runtimeEvent(`event_${cursor}`, 'read_device', {}, { cursor }),
        }),
      );
    }
    await flush();

    expect(handler.calls.filter((call) => call.startsWith('event:'))).toEqual([
      'event:event_1',
      'event:event_2',
      'event:event_3',
      'event:event_1',
    ]);
    close();
  });

  it('aborts reconnect sleep and reader work on close', async () => {
    vi.useFakeTimers();
    const mock = vi.fn(() => Promise.reject(new Error('offline')));
    vi.stubGlobal('fetch', mock);
    const close = connectRuntimeStream({
      workspaceId: 'ws',
      taskId: 'task',
      callbacks: callbacks(),
      jitterMs: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    close();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mock).toHaveBeenCalledTimes(1);
  });
});
