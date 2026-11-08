/**
 * Harness 唯一请求入口。
 *
 * 组件不得直接 fetch；异常处理、Supervisor、压缩和协调数据都从这里进入。
 * 后端 DTO 使用 camelCase，ApiClient 仍统一做 snake_case 参数与 camelCase 响应转换。
 */

import { ApiClient, type ApiResponse } from './client.js';
import type {
  HarnessCompactionComparison,
  HarnessOverview,
  HarnessProjectionSnapshot,
  HarnessSubagentPage,
  HarnessSupervisor,
} from '../types/harness.js';

function harnessPath(workspaceId: string, suffix = ''): string {
  return `/workspaces/${encodeURIComponent(workspaceId)}/harness${suffix}`;
}

export interface HarnessOverviewQuery {
  attentionLimit?: number;
  attentionOffset?: number;
}

/** Exception-first overview: attention page and current Main/Subagent rows. */
export function getHarnessOverview(
  workspaceId: string,
  query: HarnessOverviewQuery = {},
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessOverview>> {
  return ApiClient.get(harnessPath(workspaceId, '/overview'), query, signal);
}

/**
 * Bounded child page for one Main Task. Subagents are loaded on expand so a
 * Main with many children never renders them all at once.
 */
export function listHarnessSubagents(
  workspaceId: string,
  mainTaskId: string,
  limit = 20,
  offset = 0,
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessSubagentPage>> {
  return ApiClient.get(
    harnessPath(workspaceId, `/main/${encodeURIComponent(mainTaskId)}/subagents`),
    { limit, offset },
    signal,
  );
}

/**
 * Read-only Supervisor health facts plus the current dead-wake page. Retry
 * permission is reported per caller; no heartbeat is used as liveness.
 */
export function getHarnessSupervisor(
  workspaceId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessSupervisor>> {
  return ApiClient.get(harnessPath(workspaceId, '/supervisor'), undefined, signal);
}

/**
 * Confirm exactly one original dead wake. The outbox identity is
 * `(task_id, event_id)`; callers never pass a replacement or bulk list.
 */
export function retryHarnessDeadWake(
  workspaceId: string,
  taskId: string,
  eventId: string,
): Promise<ApiResponse<{ taskId: string; eventId: string; state: string; retriedBy: string }>> {
  return ApiClient.post(
    harnessPath(
      workspaceId,
      `/supervisor/dead-wakes/${encodeURIComponent(taskId)}/${encodeURIComponent(eventId)}/retry`,
    ),
    {},
  );
}

/** Fixed boundary shared by the temporary rebuild and the current snapshot. */
export interface HarnessCompactionQuery {
  projectionName: string;
  subjectId: string;
  lastSeq: number;
  projectionVersion: number;
}

/**
 * Rebuild a temporary projection at the fixed boundary and compare canonical
 * hashes. Read-only; `canReplace` is true only for an exact match *and* an
 * Agent-management caller.
 */
export function compareHarnessCompaction(
  workspaceId: string,
  query: HarnessCompactionQuery,
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessCompactionComparison>> {
  return ApiClient.get(harnessPath(workspaceId, '/compaction'), query, signal);
}

export interface HarnessCompactionReplace extends HarnessCompactionQuery {
  /** Observed boundary and state version used as the CAS precondition. */
  expectedLastSeq: number;
  expectedStateVersion: number;
}

/**
 * Explicit CAS replacement. The server re-runs the comparison; a mismatch or
 * concurrent version change is a 409 and keeps the latest snapshot.
 */
export function replaceHarnessCompactionSnapshot(
  workspaceId: string,
  request: HarnessCompactionReplace,
): Promise<ApiResponse<HarnessProjectionSnapshot>> {
  return ApiClient.post(harnessPath(workspaceId, '/compaction/replace'), request);
}
