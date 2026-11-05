/**
 * Harness 唯一请求入口。
 *
 * 组件不得直接 fetch；Workflow、异常处理和协调数据都从这里进入。
 * 后端 DTO 使用 camelCase，ApiClient 仍统一做 snake_case 参数与 camelCase 响应转换。
 */

import { ApiClient, type ApiResponse } from './client.js';
import type { HarnessOverview, HarnessSubagentPage } from '../types/harness.js';

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
