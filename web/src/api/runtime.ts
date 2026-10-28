import { ApiClient, type ApiResponse } from './client.js';
import type {
  CreateTaskResult,
  RuntimeApprovalDecisionResult,
  RuntimeTaskListResult,
  TaskProjection,
} from '../types/runtime.js';

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
  return ApiClient.post(taskPath(workspaceId), { text }, {
    headers: { 'Idempotency-Key': idempotencyKey },
    signal,
  });
}

export function listRuntimeTasks(
  workspaceId: string,
  limit = 50,
  offset = 0,
  signal?: AbortSignal,
): Promise<ApiResponse<RuntimeTaskListResult>> {
  return ApiClient.get(taskPath(workspaceId), { limit, offset }, signal);
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
