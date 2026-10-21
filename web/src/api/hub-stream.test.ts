import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  buildHubStreamPath,
  connectHubStream,
  fetchDeclaredSources,
  parseHubEvent,
  type HubStreamStatus,
} from './hub-stream.js';
import type { SseEvent } from './live.js';

function frame(event: string, data: unknown): string {
  return `event: ${event}\ndata: ${typeof data === 'string' ? data : JSON.stringify(data)}\n\n`;
}

interface ControlledResponse {
  response: Response;
  push: (text: string) => void;
  error: () => void;
}

function controlledResponse(): ControlledResponse {
  const encoder = new TextEncoder();
  let controller: ReadableStreamDefaultController<Uint8Array> | null = null;
  let finished = false;
  const body = new ReadableStream<Uint8Array>({
    start(c) {
      controller = c;
    },
  });
  return {
    response: new Response(body, { status: 200 }),
    push(text) {
      controller?.enqueue(encoder.encode(text));
    },
    error() {
      if (finished) return;
      finished = true;
      controller?.error(new Error('aborted'));
    },
  };
}

function streamingFetch() {
  const streams: ControlledResponse[] = [];
  const mock = vi.fn((_url: string, init?: RequestInit) => {
    const ctl = controlledResponse();
    streams.push(ctl);
    init?.signal?.addEventListener('abort', () => ctl.error());
    return Promise.resolve(ctl.response);
  });
  return { mock, streams };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
  sessionStorage.setItem('auth-token', 'test-token');
});

afterEach(() => {
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('buildHubStreamPath', () => {
  it('encodes workspace/things/sources and keeps commas as separators', () => {
    expect(buildHubStreamPath('ws 1', ['thing/1', ' t2 ', 'thing/1'], ['telemetry', 'pos&x'])).toBe(
      '/workspaces/ws%201/hub/stream?things=thing%2F1,t2&sources=telemetry,pos%26x',
    );
  });
});

describe('parseHubEvent', () => {
  const ev = (event: string, data: unknown): SseEvent => ({
    event,
    data: typeof data === 'string' ? data : JSON.stringify(data),
  });

  it('parses snapshot with per-source values', () => {
    const parsed = parseHubEvent(
      ev('snapshot', {
        v: 1,
        devices: [
          {
            thingId: 't1',
            name: 'meter',
            sources: [
              {
                source: 'telemetry',
                values: { temp: '22' },
                lastHeartbeatAt: '2026-10-21T00:00:00Z',
              },
            ],
          },
        ],
      }),
    );
    expect(parsed).toEqual({
      type: 'snapshot',
      data: {
        v: 1,
        devices: [
          {
            thingId: 't1',
            name: 'meter',
            sources: [
              {
                source: 'telemetry',
                values: { temp: '22' },
                lastHeartbeatAt: '2026-10-21T00:00:00Z',
              },
            ],
          },
        ],
      },
    });
  });

  it('parses reading/control/resync and ping', () => {
    expect(
      parseHubEvent(
        ev('reading', { v: 1, ts: 't', thingId: 't1', source: 'telemetry', values: { temp: 23 } }),
      ),
    ).toEqual({
      type: 'reading',
      data: { v: 1, ts: 't', thingId: 't1', source: 'telemetry', values: { temp: 23 } },
    });
    expect(
      parseHubEvent(
        ev('control', { v: 1, ts: 't', thingId: 't1', kind: 'job', payload: { id: 'j1' } }),
      ),
    ).toEqual({
      type: 'control',
      data: { v: 1, ts: 't', thingId: 't1', kind: 'job', payload: { id: 'j1' } },
    });
    expect(parseHubEvent(ev('resync', { v: 1, reason: 'lagged' }))).toEqual({
      type: 'resync',
      data: { v: 1, reason: 'lagged' },
    });
    expect(parseHubEvent(ev('ping', {}))).toEqual({ type: 'ping' });
  });

  it('ignores invalid JSON and malformed payloads', () => {
    expect(parseHubEvent(ev('reading', 'not-json'))).toBeNull();
    expect(parseHubEvent(ev('reading', { thingId: 't1' }))).toBeNull();
    expect(parseHubEvent(ev('unknown', { v: 1 }))).toBeNull();
  });
});

describe('connectHubStream', () => {
  it('sends Bearer auth and dispatches typed frames (including split chunks)', async () => {
    const { mock, streams } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    const snapshots: unknown[] = [];
    const readings: unknown[] = [];
    const controls: unknown[] = [];
    const resyncs: string[] = [];
    let pings = 0;

    const conn = connectHubStream(
      'ws1',
      ['t1'],
      ['telemetry', 'position'],
      {
        onStatus: () => {},
        onSnapshot: (s) => snapshots.push(s),
        onReading: (r) => readings.push(r),
        onControl: (c) => controls.push(c),
        onResync: (r) => resyncs.push(r.reason),
        onPing: () => (pings += 1),
      },
      { pauseOnHidden: false },
    );
    await flush();

    expect(mock).toHaveBeenCalledTimes(1);
    const [url, init] = mock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/workspaces/ws1/hub/stream?things=t1&sources=telemetry,position');
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer test-token');
    expect(init.signal).toBeInstanceOf(AbortSignal);

    streams[0].push(
      frame('snapshot', {
        v: 1,
        devices: [
          {
            thingId: 't1',
            name: 'meter',
            sources: [{ source: 'telemetry', values: { temp: '22' }, lastHeartbeatAt: null }],
          },
        ],
      }),
    );
    // 半帧跨 chunk：解析器必须保留未完成尾部。
    streams[0].push('event: reading\ndata: {"v":1,"ts":"t1","thingId":"t1","source":"tele');
    await flush();
    streams[0].push('metry","values":{"temp":23}}\n\n');
    streams[0].push(
      frame('control', { v: 1, ts: 't2', thingId: 't1', kind: 'job', payload: { id: 'j1' } }),
    );
    streams[0].push(frame('ping', {}));
    streams[0].push(frame('resync', { v: 1, reason: 'lagged' }));
    await flush();

    expect(snapshots).toHaveLength(1);
    expect(readings).toEqual([
      { v: 1, ts: 't1', thingId: 't1', source: 'telemetry', values: { temp: 23 } },
    ]);
    expect(controls).toHaveLength(1);
    expect(pings).toBe(1);
    expect(resyncs).toEqual(['lagged']);

    conn.close();
  });

  it('reconnects with exponential backoff capped at maxDelay', async () => {
    vi.useFakeTimers();
    const mock = vi.fn(() => Promise.reject(new Error('down')));
    vi.stubGlobal('fetch', mock);
    const statuses: HubStreamStatus[] = [];
    const conn = connectHubStream(
      'ws',
      ['t1'],
      ['telemetry'],
      { onStatus: (status) => statuses.push(status) },
      { jitterMs: 0, baseDelayMs: 1000, maxDelayMs: 4000, pauseOnHidden: false },
    );

    await vi.advanceTimersByTimeAsync(0);
    expect(mock).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(mock).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(2000);
    expect(mock).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(4000);
    expect(mock).toHaveBeenCalledTimes(4);
    // 封顶后维持 maxDelay（不是继续翻倍到 8s）。
    await vi.advanceTimersByTimeAsync(3999);
    expect(mock).toHaveBeenCalledTimes(4);
    await vi.advanceTimersByTimeAsync(1);
    expect(mock).toHaveBeenCalledTimes(5);

    expect(statuses[0]).toBe('connecting');
    expect(statuses).toContain('reconnecting');
    conn.close();
    expect(statuses.at(-1)).toBe('offline');
  });

  it('pause aborts the in-flight request and stops reconnecting until resume', async () => {
    vi.useFakeTimers();
    const mock = vi.fn(() => Promise.reject(new Error('down')));
    vi.stubGlobal('fetch', mock);
    const conn = connectHubStream(
      'ws',
      ['t1'],
      ['telemetry'],
      { onStatus: () => {} },
      { jitterMs: 0, pauseOnHidden: false },
    );

    await vi.advanceTimersByTimeAsync(0);
    expect(mock).toHaveBeenCalledTimes(1);
    conn.pause();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mock).toHaveBeenCalledTimes(1);

    conn.resume();
    await vi.advanceTimersByTimeAsync(0);
    expect(mock).toHaveBeenCalledTimes(2);

    conn.close();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(mock).toHaveBeenCalledTimes(2);
  });

  it('pauses while hidden and resumes when visible', async () => {
    const { mock } = streamingFetch();
    vi.stubGlobal('fetch', mock);
    const conn = connectHubStream('ws', ['t1'], ['telemetry'], { onStatus: () => {} });
    await flush();
    expect(mock).toHaveBeenCalledTimes(1);
    const firstSignal = (mock.mock.calls[0] as unknown as [string, RequestInit])[1].signal!;

    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
    document.dispatchEvent(new Event('visibilitychange'));
    expect(firstSignal.aborted).toBe(true);
    await flush();

    Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
    document.dispatchEvent(new Event('visibilitychange'));
    await flush();
    expect(mock).toHaveBeenCalledTimes(2);

    conn.close();
    delete (document as unknown as { hidden?: boolean }).hidden;
  });
});

describe('fetchDeclaredSources', () => {
  it('reads source ids from the driver declaration with Bearer auth', async () => {
    const mock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          success: true,
          data: { sources: [{ id: 'telemetry' }, { id: 'position' }, { nope: true }] },
        }),
        { status: 200 },
      ),
    );
    vi.stubGlobal('fetch', mock);

    await expect(fetchDeclaredSources('demo')).resolves.toEqual(['telemetry', 'position']);
    const [url, init] = mock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/v1/drivers/demo/declaration');
    expect(new Headers(init.headers).get('Authorization')).toBe('Bearer test-token');
  });

  it('returns an empty list when declaration is unavailable', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('nope', { status: 404 })));
    await expect(fetchDeclaredSources('missing')).resolves.toEqual([]);
    await expect(fetchDeclaredSources('')).resolves.toEqual([]);
  });
});
