/**
 * E3 实时投影 SSE 客户端（spec 2026-10-21-e3 §3.2）。
 *
 * EventSource 不能带 Authorization header，因此用 fetch + ReadableStream
 * 手写 SSE 解析；token 不进 URL。支持 Last-Event-ID 重连与指数退避。
 */

import { buildUrl, getAuthToken } from './client.js';

export interface SseEvent {
  event: string;
  data: string;
  id?: string;
}

export type LiveStatus = 'connecting' | 'open' | 'closed' | 'error';

export interface LiveHandlers {
  onEvent: (ev: SseEvent) => void;
  onStatus: (status: LiveStatus) => void;
}

export interface LiveConnection {
  close: () => void;
  pause: () => void;
  resume: () => void;
}

/**
 * 解析 SSE 文本块：返回完整事件与未完成的尾部。兼容 CRLF/多行 data/注释。
 */
export function parseSseChunk(buffer: string): { events: SseEvent[]; rest: string } {
  // 统一换行，按空行分帧；最后一个不完整帧留在 rest。
  const normalized = buffer.replace(/\r\n/g, '\n');
  const frames = normalized.split('\n\n');
  const rest = frames.pop() ?? '';
  const events: SseEvent[] = [];
  for (const frame of frames) {
    if (!frame.trim()) continue;
    let event = 'message';
    let id: string | undefined;
    const data: string[] = [];
    for (const line of frame.split('\n')) {
      if (!line || line.startsWith(':')) continue; // 注释/keepalive
      const idx = line.indexOf(':');
      const field = idx === -1 ? line : line.slice(0, idx);
      const value = idx === -1 ? '' : line.slice(idx + 1).replace(/^ /, '');
      if (field === 'event') event = value;
      else if (field === 'id') id = value;
      else if (field === 'data') data.push(value);
    }
    if (data.length > 0) events.push({ event, data: data.join('\n'), id });
  }
  return { events, rest };
}

function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    signal.addEventListener('abort', () => {
      clearTimeout(timer);
      resolve();
    }, { once: true });
  });
}

export function connectLive(
  workspaceId: string,
  handlers: LiveHandlers,
  options: { baseDelayMs?: number; maxDelayMs?: number } = {},
): LiveConnection {
  const baseDelay = options.baseDelayMs ?? 1000;
  const maxDelay = options.maxDelayMs ?? 30_000;
  let closed = false;
  let paused = false;
  let controller: AbortController | null = null;
  let lastEventId: string | null = null;
  let retry = baseDelay;
  let running = false;

  const run = async () => {
    if (running) return;
    running = true;
    while (!closed && !paused) {
      controller = new AbortController();
      handlers.onStatus('connecting');
      try {
        const headers: Record<string, string> = { Accept: 'text/event-stream' };
        const token = getAuthToken();
        if (token) headers.Authorization = `Bearer ${token}`;
        if (lastEventId) headers['Last-Event-ID'] = lastEventId;
        const res = await fetch(
          buildUrl(`/workspaces/${encodeURIComponent(workspaceId)}/live/stream`),
          { headers, signal: controller.signal },
        );
        if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
        handlers.onStatus('open');
        retry = baseDelay;
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        while (!closed && !paused) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const { events, rest } = parseSseChunk(buffer);
          buffer = rest;
          for (const ev of events) {
            if (ev.id) lastEventId = ev.id;
            handlers.onEvent(ev);
          }
        }
      } catch {
        if (closed || paused) break;
        handlers.onStatus('error');
      }
      if (closed || paused) break;
      handlers.onStatus('closed');
      const jitter = Math.floor(Math.random() * 250);
      const aborted = new AbortController();
      await sleep(retry + jitter, aborted.signal);
      retry = Math.min(retry * 2, maxDelay);
    }
    running = false;
  };

  void run();

  return {
    close() {
      closed = true;
      controller?.abort();
    },
    pause() {
      paused = true;
      controller?.abort();
    },
    resume() {
      if (closed) return;
      paused = false;
      retry = baseDelay;
      void run();
    },
  };
}
