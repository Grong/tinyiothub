/**
 * Agent run trace API（AI 运行透明化 T11，/agent-runs 域）。
 *
 * 契约（T5 已上线）：GET /agent-runs/{run_id}/trace
 *   → { live, budgetMax, entries: [{ id, name, args, output }] }
 * 后端 DTO `#[serde(rename_all = "camelCase")]`，前端类型直接写 camelCase
 * （web-case-converter 类型/运行时不对称教训：惯例 = 直接 camelCase）。
 * 三分支语义：live=进行中（内存 trace）/ !live=已结束（report JSON 列）/ 404=不存在。
 */

import { apiGet, ApiError } from "./client.js";

/** 一次工具调用轨迹；output=null = 未配对 ToolResult（进行中/被取消）。 */
export interface TraceEntry {
  id: string;
  name: string;
  args: unknown;
  output: string | null;
}

export interface RunTrace {
  live: boolean;
  budgetMax: number;
  entries: TraceEntry[];
}

/** 404 —— run 不存在（含跨 workspace 命中，后端不泄漏存在性）。 */
export class TraceNotFoundError extends Error {
  constructor(public readonly runId: string) {
    super(`run 不存在: ${runId}`);
    this.name = "TraceNotFoundError";
  }
}

/** 拉取 run 工具轨迹；404 抛 TraceNotFoundError，其余 ApiError 原样透传。 */
export async function fetchRunTrace(runId: string): Promise<RunTrace> {
  try {
    const res = await apiGet<RunTrace>(`/agent-runs/${encodeURIComponent(runId)}/trace`);
    if (res.result === null) throw new Error("空响应");
    return res.result;
  } catch (e) {
    if (e instanceof ApiError && e.code === 404) throw new TraceNotFoundError(runId);
    throw e;
  }
}
