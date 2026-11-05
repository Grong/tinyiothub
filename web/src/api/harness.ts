/**
 * Harness 唯一请求入口。
 *
 * 组件不得直接 fetch；Workflow、异常处理和协调数据都从这里进入。
 * 后端 DTO 使用 camelCase，ApiClient 仍统一做 snake_case 参数与 camelCase 响应转换。
 */

import { ApiClient, type ApiResponse } from './client.js';
import type {
  HarnessDryRunResult,
  HarnessOverview,
  HarnessSimulationProjection,
  HarnessSubagentPage,
  HarnessSupervisor,
  HarnessWorkflowSnapshotList,
  HarnessWorkflowTemplateInput,
  HarnessWorkflowTemplateList,
  WorkflowDefinition,
  WorkflowRevision,
} from '../types/harness.js';

function harnessPath(workspaceId: string, suffix = ''): string {
  return `/workspaces/${encodeURIComponent(workspaceId)}/harness${suffix}`;
}

export interface HarnessOverviewQuery {
  attentionLimit?: number;
  attentionOffset?: number;
}

/** Exception-first overview: attention page, Workflows and current Main/Subagent rows. */
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

function workflowPath(workspaceId: string, workflowId?: string, suffix = ''): string {
  const base = harnessPath(workspaceId, '/workflows');
  return workflowId ? `${base}/${encodeURIComponent(workflowId)}${suffix}` : base;
}

/** Current revision of one Workflow. */
export function getHarnessWorkflow(
  workspaceId: string,
  workflowId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<WorkflowRevision>> {
  return ApiClient.get(workflowPath(workspaceId, workflowId), undefined, signal);
}

/**
 * Create a new Workflow revision. The payload deliberately carries no enabled
 * flag: a blank create is persisted disabled and enabling is a separate action.
 */
export function createHarnessWorkflow(
  workspaceId: string,
  definition: WorkflowDefinition,
): Promise<ApiResponse<WorkflowRevision>> {
  return ApiClient.post(workflowPath(workspaceId), definition);
}

export interface HarnessWorkflowUpdate {
  /** Optimistic-concurrency token; a mismatch is reported as 409/Conflict. */
  expectedVersion: number;
  /** Current value is echoed so editing never changes enablement (R30). */
  enabled: boolean;
  definition: WorkflowDefinition;
}

/** Append a new revision while preserving the current enabled/disabled state. */
export function updateHarnessWorkflow(
  workspaceId: string,
  workflowId: string,
  update: HarnessWorkflowUpdate,
): Promise<ApiResponse<WorkflowRevision>> {
  return ApiClient.put(workflowPath(workspaceId, workflowId), update);
}

/** Explicit enable/disable action; never called from save. */
export function setHarnessWorkflowEnabled(
  workspaceId: string,
  workflowId: string,
  enabled: boolean,
): Promise<ApiResponse<WorkflowRevision>> {
  return ApiClient.put(workflowPath(workspaceId, workflowId, '/enabled'), { enabled });
}

/** Built-in inspection/video/energy templates with their full definitions. */
export function listHarnessWorkflowTemplates(
  workspaceId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessWorkflowTemplateList>> {
  return ApiClient.get(harnessPath(workspaceId, '/workflow-templates'), undefined, signal);
}

/**
 * DR4=B: validated instantiation creates an enabled Workflow directly. A
 * rejected request must not create a Workflow or schedule anything.
 */
export function instantiateHarnessWorkflowTemplate(
  workspaceId: string,
  templateId: string,
  input: HarnessWorkflowTemplateInput,
): Promise<ApiResponse<WorkflowRevision>> {
  return ApiClient.post(
    harnessPath(workspaceId, `/workflow-templates/${encodeURIComponent(templateId)}/instantiate`),
    input,
  );
}

function workflowVersionPath(workspaceId: string, workflowId: string, version: number, suffix: string): string {
  return harnessPath(
    workspaceId,
    `/workflows/${encodeURIComponent(workflowId)}/versions/${version}${suffix}`,
  );
}

/** One fixed historical revision for the dry-run return path. */
export function getHarnessWorkflowRevision(
  workspaceId: string,
  workflowId: string,
  version: number,
  signal?: AbortSignal,
): Promise<ApiResponse<WorkflowRevision>> {
  return ApiClient.get(workflowVersionPath(workspaceId, workflowId, version, ''), undefined, signal);
}

/** Historical production snapshots for the fixed-revision dry-run picker. */
export function listHarnessWorkflowSnapshots(
  workspaceId: string,
  workflowId: string,
  version: number,
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessWorkflowSnapshotList>> {
  return ApiClient.get(workflowVersionPath(workspaceId, workflowId, version, '/snapshots'), undefined, signal);
}

/**
 * Start an isolated simulation. The caller must have Agent management
 * permission; the chosen snapshot is fixed for the whole Run.
 */
export function startHarnessDryRun(
  workspaceId: string,
  workflowId: string,
  version: number,
  snapshotSeq: number,
): Promise<ApiResponse<HarnessDryRunResult>> {
  return ApiClient.post(workflowVersionPath(workspaceId, workflowId, version, '/dry-run'), {
    snapshotSeq,
  });
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
 * Confirm exactly one original dead wake. The server reuses the original
 * event ID; callers never pass a replacement or bulk list.
 */
export function retryHarnessDeadWake(
  workspaceId: string,
  eventId: string,
): Promise<ApiResponse<{ eventId: string; state: string; retriedBy: string }>> {
  return ApiClient.post(
    harnessPath(workspaceId, `/supervisor/dead-wakes/${encodeURIComponent(eventId)}/retry`),
    {},
  );
}

/** Read-only simulation Run projection; production Task IDs are rejected. */
export function getHarnessSimulation(
  workspaceId: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<ApiResponse<HarnessSimulationProjection>> {
  return ApiClient.get(
    harnessPath(workspaceId, `/simulations/${encodeURIComponent(taskId)}`),
    undefined,
    signal,
  );
}
