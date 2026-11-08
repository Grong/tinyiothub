import { ApiClient, type ApiResponse } from './client.js';
import type {
  CreateTaskResult,
  RuntimeApprovalDecisionResult,
  RuntimeEvent,
  RuntimeEventsPage,
  RuntimeTaskListResult,
  TaskProjection,
} from '../types/runtime.js';

/** Harness/SessionEvent wire row (`seq` / `eventType` / `payloadJson`). */
interface HarnessEventWire {
  seq: number;
  eventId: string;
  eventType: string;
  occurredAt: string;
  workspaceId: string;
  taskId?: string | null;
  runId?: string | null;
  payloadJson?: Record<string, unknown> | null;
}

interface HarnessEventsPageWire {
  events: HarnessEventWire[];
  oldestCursor: number | null;
  hasMore: boolean;
}

function str(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function int(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
}

/** Transitional projection for the Batch-5 runtime views. */
function toRuntimeEvent(wire: HarnessEventWire): RuntimeEvent {
  const payload = wire.payloadJson ?? {};
  return {
    cursor: wire.seq,
    eventId: wire.eventId,
    eventType: wire.eventType,
    occurredAt: wire.occurredAt,
    workspaceId: wire.workspaceId,
    taskId: wire.taskId ?? null,
    runId: wire.runId ?? null,
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

function taskPath(workspaceId: string, taskId?: string): string {
  const base = `/workspaces/${encodeURIComponent(workspaceId)}/agent/tasks`;
  return taskId === undefined ? base : `${base}/${encodeURIComponent(taskId)}`;
}

export function createRuntimeTask(
  workspaceId: string,
  text: string,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<ApiResponse<CreateTaskResult>> {
  return ApiClient.post(
    taskPath(workspaceId),
    { text },
    {
      headers: { 'Idempotency-Key': idempotencyKey },
      signal,
    },
  );
}

export function listRuntimeTasks(
  workspaceId: string,
  limit = 50,
  offset = 0,
  signal?: AbortSignal,
): Promise<ApiResponse<RuntimeTaskListResult>> {
  return ApiClient.get(taskPath(workspaceId), { limit, offset }, signal);
}

/**
 * Bounded Task timeline page. `before` is exclusive; omitted means the newest
 * page. The server caps `limit` at 200 and reports `oldestCursor`/`hasMore`.
 */
export async function listRuntimeTaskEvents(
  workspaceId: string,
  taskId: string,
  limit = 100,
  before?: number,
  signal?: AbortSignal,
): Promise<ApiResponse<RuntimeEventsPage>> {
  const response = await ApiClient.get<HarnessEventsPageWire>(
    `${taskPath(workspaceId, taskId)}/events`,
    before === undefined ? { limit } : { limit, before },
    signal,
  );
  if (response.result === null || response.result === undefined) {
    return response as unknown as ApiResponse<RuntimeEventsPage>;
  }
  return {
    ...response,
    result: {
      events: response.result.events.map(toRuntimeEvent),
      oldestCursor: response.result.oldestCursor,
      hasMore: response.result.hasMore,
    },
  };
}

export function getRuntimeTask(
  workspaceId: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<TaskProjection>> {
  return ApiClient.get(taskPath(workspaceId, taskId), undefined, signal);
}

function controlRuntimeTask(
  workspaceId: string,
  taskId: string,
  action: 'pause' | 'resume' | 'cancel',
  signal?: AbortSignal,
): Promise<ApiResponse<TaskProjection>> {
  return ApiClient.post(`${taskPath(workspaceId, taskId)}/${action}`, {}, { signal });
}

export function pauseRuntimeTask(
  workspaceId: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<TaskProjection>> {
  return controlRuntimeTask(workspaceId, taskId, 'pause', signal);
}

export function resumeRuntimeTask(
  workspaceId: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<TaskProjection>> {
  return controlRuntimeTask(workspaceId, taskId, 'resume', signal);
}

export function cancelRuntimeTask(
  workspaceId: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<TaskProjection>> {
  return controlRuntimeTask(workspaceId, taskId, 'cancel', signal);
}

export function stopRuntimeJob(
  workspaceId: string,
  taskId: string,
  jobId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<TaskProjection>> {
  return ApiClient.post(
    `${taskPath(workspaceId, taskId)}/jobs/${encodeURIComponent(jobId)}/stop`,
    {},
    { signal },
  );
}

export function decideRuntimeApproval(
  workspaceId: string,
  taskId: string,
  approvalId: string,
  decision: 'approve' | 'reject',
  signal?: AbortSignal,
): Promise<ApiResponse<RuntimeApprovalDecisionResult>> {
  return ApiClient.post(
    `${taskPath(workspaceId, taskId)}/approvals/${encodeURIComponent(approvalId)}/${decision}`,
    {},
    { signal },
  );
}

export class RuntimeCreateModel {
  private objective: string | null = null;
  private key = '';

  keyFor(objective: string): string {
    if (objective !== this.objective || !this.key) {
      this.objective = objective;
      this.key = crypto.randomUUID();
    }
    return this.key;
  }

  reset(): void {
    this.objective = null;
    this.key = '';
  }
}
