/**
 * E4b Hub 订阅流 SSE 客户端（spec 2026-10-21-e4b §2.4）。
 *
 * `GET /v1/workspaces/{id}/hub/stream?things=&sources=` 按设备/数据源订阅：
 * 首帧 `snapshot`，增量 `reading`（latest-wins）、`control`（有序）、
 * `ping`（15s）与 `resync`（lagged/overflow/reconnect）。
 *
 * 与 E3 `/live/stream` 一样用 fetch + ReadableStream 手写 SSE 解析：
 * EventSource 不能带 Authorization header，token 不进 URL。断线按
 * 1s→30s 指数退避重连；页面隐藏时暂停，可见时恢复。
 */

import { buildUrl, getAuthToken } from './client.js';
import { parseSseChunk, type SseEvent } from './live.js';

export type HubStreamStatus = 'connecting' | 'live' | 'reconnecting' | 'offline';

export interface HubSnapshotSource {
  source: string;
  values: Record<string, unknown>;
  /** RFC3339；后端未提供时为 null。 */
  lastHeartbeatAt: string | null;
}

export interface HubSnapshotDevice {
  thingId: string;
  name: string;
  sources: HubSnapshotSource[];
}

export interface HubSnapshot {
  v: number;
  devices: HubSnapshotDevice[];
}

export interface HubReading {
  v: number;
  ts: string;
  thingId: string;
  source: string;
  values: Record<string, unknown>;
}

export interface HubControl {
  v: number;
  ts: string;
  thingId: string;
  kind: string;
  payload: Record<string, unknown>;
}

export interface HubResync {
  v: number;
  reason: string;
}

export type HubEvent =
  | { type: 'snapshot'; data: HubSnapshot }
  | { type: 'reading'; data: HubReading }
  | { type: 'control'; data: HubControl }
  | { type: 'resync'; data: HubResync }
  | { type: 'ping' };

export interface HubStreamHandlers {
  onStatus: (status: HubStreamStatus) => void;
  onSnapshot?: (snapshot: HubSnapshot) => void;
  onReading?: (reading: HubReading) => void;
  onControl?: (control: HubControl) => void;
  onResync?: (resync: HubResync) => void;
  onPing?: () => void;
}

export interface HubConnection {
  close: () => void;
  pause: () => void;
  resume: () => void;
}

export interface HubStreamOptions {
  baseDelayMs?: number;
  maxDelayMs?: number;
  /** 退避随机抖动上限；测试可传 0。 */
  jitterMs?: number;
  /** 默认 true：document.hidden 时暂停、可见时恢复。 */
  pauseOnHidden?: boolean;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function parseJson(data: string): Record<string, unknown> | null {
  try {
    return asRecord(JSON.parse(data));
  } catch {
    return null;
  }
}

function parseSnapshot(raw: Record<string, unknown>): HubSnapshot | null {
  if (!Array.isArray(raw.devices)) return null;
  const devices: HubSnapshotDevice[] = [];
  for (const entry of raw.devices) {
    const device = asRecord(entry);
    if (!device || typeof device.thingId !== 'string') continue;
    const sources: HubSnapshotSource[] = [];
    if (Array.isArray(device.sources)) {
      for (const item of device.sources) {
        const source = asRecord(item);
        if (!source || typeof source.source !== 'string') continue;
        sources.push({
          source: source.source,
          values: asRecord(source.values) ?? {},
          lastHeartbeatAt:
            typeof source.lastHeartbeatAt === 'string' ? source.lastHeartbeatAt : null,
        });
      }
    }
    devices.push({
      thingId: device.thingId,
      name: typeof device.name === 'string' ? device.name : device.thingId,
      sources,
    });
  }
  return { v: typeof raw.v === 'number' ? raw.v : 1, devices };
}

function parseReading(raw: Record<string, unknown>): HubReading | null {
  if (typeof raw.thingId !== 'string' || typeof raw.source !== 'string') return null;
  const values = asRecord(raw.values);
  if (!values) return null;
  return {
    v: typeof raw.v === 'number' ? raw.v : 1,
    ts: typeof raw.ts === 'string' ? raw.ts : '',
    thingId: raw.thingId,
    source: raw.source,
    values,
  };
}

function parseControl(raw: Record<string, unknown>): HubControl | null {
  if (typeof raw.thingId !== 'string' || typeof raw.kind !== 'string') return null;
  return {
    v: typeof raw.v === 'number' ? raw.v : 1,
    ts: typeof raw.ts === 'string' ? raw.ts : '',
    thingId: raw.thingId,
    kind: raw.kind,
    payload: asRecord(raw.payload) ?? {},
  };
}

/** 把一条 SSE 帧解析为有类型的 Hub 事件；未知/非法帧返回 null。 */
export function parseHubEvent(ev: SseEvent): HubEvent | null {
  if (ev.event === 'ping') return { type: 'ping' };
  const raw = parseJson(ev.data);
  if (!raw) return null;
  switch (ev.event) {
    case 'snapshot': {
      const data = parseSnapshot(raw);
      return data ? { type: 'snapshot', data } : null;
    }
    case 'reading': {
      const data = parseReading(raw);
      return data ? { type: 'reading', data } : null;
    }
    case 'control': {
      const data = parseControl(raw);
      return data ? { type: 'control', data } : null;
    }
    case 'resync':
      return {
        type: 'resync',
        data: {
          v: typeof raw.v === 'number' ? raw.v : 1,
          reason: typeof raw.reason === 'string' ? raw.reason : 'unknown',
        },
      };
    default:
      return null;
  }
}

function encodeList(items: string[]): string {
  const seen = new Set<string>();
  const encoded: string[] = [];
  for (const raw of items) {
    const item = raw.trim();
    if (!item || seen.has(item)) continue;
    seen.add(item);
    encoded.push(encodeURIComponent(item));
  }
  return encoded.join(',');
}

/** 构建 Hub 流路径（含 URL 编码；逗号保留为分隔符）。 */
export function buildHubStreamPath(
  workspaceId: string,
  things: string[],
  sources: string[],
): string {
  return `/workspaces/${encodeURIComponent(workspaceId)}/hub/stream?things=${encodeList(
    things,
  )}&sources=${encodeList(sources)}`;
}

/** 读取驱动声明中的 source id（设备卡据此订阅它真正声明过的源）。 */
export async function fetchDeclaredSources(driverName: string): Promise<string[]> {
  const name = driverName.trim();
  if (!name) return [];
  try {
    const headers: Record<string, string> = {};
    const token = getAuthToken();
    if (token) headers.Authorization = `Bearer ${token}`;
    const res = await fetch(buildUrl(`/drivers/${encodeURIComponent(name)}/declaration`), {
      headers,
    });
    if (!res.ok) return [];
    const body = (await res.json()) as { data?: { sources?: unknown } };
    const declared = Array.isArray(body?.data?.sources) ? body.data.sources : [];
    const ids: string[] = [];
    for (const entry of declared) {
      const source = asRecord(entry);
      if (source && typeof source.id === 'string' && source.id) ids.push(source.id);
    }
    return ids;
  } catch {
    return [];
  }
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) {
      resolve();
      return;
    }
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', onAbort);
      resolve();
    }, ms);
    const onAbort = () => {
      clearTimeout(timer);
      resolve();
    };
    signal.addEventListener('abort', onAbort, { once: true });
  });
}

export function connectHubStream(
  workspaceId: string,
  things: string[],
  sources: string[],
  handlers: HubStreamHandlers,
  options: HubStreamOptions = {},
): HubConnection {
  const baseDelay = options.baseDelayMs ?? 1000;
  const maxDelay = options.maxDelayMs ?? 30_000;
  const jitterMs = options.jitterMs ?? 250;
  const path = buildHubStreamPath(workspaceId, things, sources);

  let closed = false;
  let paused = false;
  let running = false;
  let attempts = 0;
  let retry = baseDelay;
  let controller: AbortController | null = null;

  const deliver = (parsed: HubEvent): void => {
    switch (parsed.type) {
      case 'snapshot':
        handlers.onSnapshot?.(parsed.data);
        break;
      case 'reading':
        handlers.onReading?.(parsed.data);
        break;
      case 'control':
        handlers.onControl?.(parsed.data);
        break;
      case 'resync':
        handlers.onResync?.(parsed.data);
        break;
      case 'ping':
        handlers.onPing?.();
        break;
    }
  };

  const run = async (): Promise<void> => {
    if (running) return;
    running = true;
    while (!closed && !paused) {
      controller = new AbortController();
      const signal = controller.signal;
      handlers.onStatus(attempts === 0 ? 'connecting' : 'reconnecting');
      attempts += 1;
      try {
        // Hub 流不用 Last-Event-ID：重连即收到新 snapshot，服务端会以
        // resync("reconnect") 表达这一语义，客户端不应再带游标造成环。
        const headers: Record<string, string> = { Accept: 'text/event-stream' };
        const token = getAuthToken();
        if (token) headers.Authorization = `Bearer ${token}`;
        const res = await fetch(buildUrl(path), { headers, signal });
        if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
        handlers.onStatus('live');
        retry = baseDelay;
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let reconnectRequested = false;
        while (!closed && !paused) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const { events, rest } = parseSseChunk(buffer);
          buffer = rest;
          for (const ev of events) {
            const parsed = parseHubEvent(ev);
            if (!parsed) continue;
            // resync 表示本连接已丢帧：断开重连以重新拿首帧 snapshot。
            if (parsed.type === 'resync') reconnectRequested = true;
            deliver(parsed);
          }
          if (reconnectRequested) break;
        }
      } catch {
        if (closed || paused) break;
      }
      if (closed || paused) break;
      handlers.onStatus('reconnecting');
      const jitter = jitterMs > 0 ? Math.floor(Math.random() * jitterMs) : 0;
      await sleep(retry + jitter, signal);
      retry = Math.min(retry * 2, maxDelay);
    }
    running = false;
    // resume() 可能恰好发生在循环退出前：此时 running 仍为 true，重连被
    // 跳过，这里补一次启动，保证不会停在“已恢复但未连接”的状态。
    if (!closed && !paused) void run();
  };

  const onVisibilityChange = (): void => {
    if (document.hidden) connection.pause();
    else connection.resume();
  };

  if (options.pauseOnHidden !== false && typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', onVisibilityChange);
  }

  const connection: HubConnection = {
    close() {
      if (closed) return;
      closed = true;
      if (typeof document !== 'undefined') {
        document.removeEventListener('visibilitychange', onVisibilityChange);
      }
      controller?.abort();
      handlers.onStatus('offline');
    },
    pause() {
      if (closed || paused) return;
      paused = true;
      controller?.abort();
    },
    resume() {
      if (closed || !paused) return;
      paused = false;
      attempts = 0;
      retry = baseDelay;
      if (!running) void run();
    },
  };

  void run();
  return connection;
}
