import { buildUrl, getAuthToken, getWorkspaceId } from './client.js';
import { keysToCamelCase } from './case-converter.js';
import { parseSseChunk, type SseEvent } from './live.js';
import type { RuntimeEvent, TaskProjection } from '../types/runtime.js';

export interface RuntimeStreamCallbacks {
  snapshot(projection: TaskProjection, cursor: number, highWatermark?: number): void;
  event(event: RuntimeEvent, highWatermark?: number): void;
  resync(projection: TaskProjection, cursor: number, highWatermark?: number): void;
  connection(state: 'connecting' | 'online' | 'offline'): void;
}

export interface RuntimeStreamInput {
  workspaceId: string;
  taskId: string;
  callbacks: RuntimeStreamCallbacks;
  after?: number;
  /** Generation guard supplied by the Task owner; stale connections deliver nothing. */
  isCurrent?: () => boolean;
  baseDelayMs?: number;
  maxDelayMs?: number;
  jitterMs?: number;
  /** Replay dedupe is bounded so a long-lived task cannot grow it forever. */
  maxDedupeIds?: number;
}

const stableStreamMs = 5_000;
const defaultMaxDedupeIds = 500;

type RuntimeFrame =
  | { type: 'snapshot'; projection: TaskProjection; cursor: number; highWatermark: number }
  | { type: 'event'; event: RuntimeEvent; highWatermark: number }
  | { type: 'resync'; projection: TaskProjection; cursor: number; highWatermark: number };

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function str(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function int(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
}

/**
 * Harness (`SessionEvent`) wire shape → the RuntimeEvent shape the Batch-5
 * runtime views still consume. The server now sends `seq` / `eventType` /
 * `payloadJson`; the payload fields are lifted into the transitional
 * projection so those views keep type-checking until they are deleted.
 */
function runtimeEventFromHarness(wire: Record<string, unknown>): RuntimeEvent | null {
  const seq = int(wire.seq);
  const eventId = str(wire.eventId);
  const eventType = str(wire.eventType);
  if (seq === null || eventId === null || eventType === null) return null;
  const payload = record(wire.payloadJson) ?? {};
  return {
    cursor: seq,
    eventId,
    eventType,
    occurredAt: str(wire.occurredAt) ?? '',
    workspaceId: str(wire.workspaceId) ?? '',
    taskId: str(wire.taskId),
    runId: str(wire.runId),
    toolCallId: str(payload.toolCallId) ?? str(payload.tool_call_id),
    jobId: str(payload.jobId) ?? str(payload.job_id),
    approvalId: str(payload.approvalId) ?? str(payload.approval_id) ?? str(payload.decisionId),
    deviceId: str(payload.deviceId) ?? str(payload.device_id),
    sourceId: str(payload.sourceId) ?? str(payload.source_id),
    sourceSeq: int(payload.sourceSeq) ?? int(payload.source_seq),
    evidenceId: str(payload.evidenceId) ?? str(payload.evidence_id),
    data: payload,
  };
}

export function parseRuntimeStreamFrame(frame: SseEvent): RuntimeFrame | null {
  if (!['snapshot', 'event', 'resync'].includes(frame.event)) return null;
  try {
    const data = record(keysToCamelCase(JSON.parse(frame.data)));
    if (!data) return null;
    if (frame.event === 'event') {
      const wire = record(data.event);
      const event = wire ? runtimeEventFromHarness(wire) : null;
      if (!event) return null;
      const highWatermark =
        typeof data.highWatermark === 'number' ? data.highWatermark : event.cursor;
      return { type: 'event', event, highWatermark };
    }
    const projection = record(data.snapshot);
    if (!projection || typeof data.cursor !== 'number') return null;
    const task = record(projection.task);
    if (!task || typeof task.id !== 'string') return null;
    return {
      type: frame.event === 'snapshot' ? 'snapshot' : 'resync',
      projection: projection as unknown as TaskProjection,
      cursor: data.cursor,
      highWatermark: typeof data.highWatermark === 'number' ? data.highWatermark : data.cursor,
    };
  } catch {
    return null;
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

export function connectRuntimeStream(input: RuntimeStreamInput): () => void {
  const {
    workspaceId,
    taskId,
    callbacks,
    baseDelayMs = 1000,
    maxDelayMs = 30_000,
    jitterMs = 250,
    maxDedupeIds = defaultMaxDedupeIds,
  } = input;
  const isCurrent = input.isCurrent ?? (() => true);
  let closed = false;
  let cursor = input.after ?? 0;
  let retry = baseDelayMs;
  let controller: AbortController | null = null;
  let sleeper: AbortController | null = null;
  const eventIds = new Set<string>();
  const eventCursors = new Set<number>();

  const remember = <T>(values: Set<T>, value: T): void => {
    values.add(value);
    while (values.size > maxDedupeIds) {
      const oldest = values.values().next().value;
      if (oldest === undefined) break;
      values.delete(oldest);
    }
  };

  const active = () => !closed && isCurrent();
  const path = () =>
    `/workspaces/${encodeURIComponent(workspaceId)}/agent/tasks/${encodeURIComponent(taskId)}/stream?after=${encodeURIComponent(String(cursor))}`;

  const deliver = (frame: RuntimeFrame): void => {
    if (!active()) return;
    if (frame.type === 'event') {
      if (frame.event.taskId !== null && frame.event.taskId !== taskId) return;
      if (eventIds.has(frame.event.eventId) || eventCursors.has(frame.event.cursor)) return;
      remember(eventIds, frame.event.eventId);
      remember(eventCursors, frame.event.cursor);
      cursor = Math.max(cursor, frame.event.cursor);
      callbacks.event(frame.event, frame.highWatermark);
      return;
    }
    if (frame.projection.task.id !== taskId) return;
    // A snapshot is newer than the replay that follows it; only delivered events
    // acknowledge durable replay progress. Resync is an authoritative cursor reset.
    if (frame.type === 'resync') cursor = frame.cursor;
    eventIds.clear();
    eventCursors.clear();
    if (frame.type === 'snapshot')
      callbacks.snapshot(frame.projection, frame.cursor, frame.highWatermark);
    else callbacks.resync(frame.projection, frame.cursor, frame.highWatermark);
  };

  const run = async (): Promise<void> => {
    while (active()) {
      callbacks.connection('connecting');
      controller = new AbortController();
      const signal = controller.signal;
      let opened = false;
      let openedAt = 0;
      try {
        const headers: Record<string, string> = { Accept: 'text/event-stream' };
        const token = getAuthToken();
        const headerWorkspaceId = getWorkspaceId();
        if (token) headers.Authorization = `Bearer ${token}`;
        if (headerWorkspaceId) headers['X-Workspace-Id'] = headerWorkspaceId;
        const response = await fetch(buildUrl(path()), {
          headers,
          credentials: 'include',
          signal,
        });
        if (!active()) break;
        if (!response.ok || !response.body) {
          try {
            await response.body?.cancel();
          } catch {
            // The response body may already be closed.
          }
          controller.abort();
          // Authentication cannot heal by retrying; stop instead of hammering.
          if (response.status === 401 || response.status === 403) {
            closed = true;
            callbacks.connection('offline');
            return;
          }
          throw new Error(`HTTP ${response.status}`);
        }
        opened = true;
        openedAt = Date.now();
        callbacks.connection('online');
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        try {
          while (active()) {
            const { done, value } = await reader.read();
            if (done) break;
            buffer += decoder.decode(value, { stream: true });
            const parsed = parseSseChunk(buffer);
            buffer = parsed.rest;
            for (const raw of parsed.events) {
              if (!active()) break;
              const frame = parseRuntimeStreamFrame(raw);
              if (frame) deliver(frame);
            }
          }
        } finally {
          controller.abort();
          try {
            await reader.cancel();
          } catch {
            // The aborted response body is already closed.
          }
        }
      } catch {
        if (!active()) break;
      }
      if (!active()) break;
      if (!opened) callbacks.connection('offline');
      // Only a stream that stayed healthy for a while proves the endpoint is
      // stable enough to reset backoff; flapping connections keep escalating.
      if (opened && Date.now() - openedAt >= stableStreamMs) retry = baseDelayMs;
      const jitter = jitterMs > 0 ? Math.floor(Math.random() * jitterMs) : 0;
      sleeper = new AbortController();
      await sleep(retry + jitter, sleeper.signal);
      sleeper = null;
      retry = Math.min(retry * 2, maxDelayMs);
    }
  };

  void run();
  return () => {
    if (closed) return;
    closed = true;
    controller?.abort();
    sleeper?.abort();
    if (isCurrent()) callbacks.connection('offline');
  };
}
