/**
 * Agent run trace contract（AI 运行透明化 T11）：`GET /agent-runs/{run_id}/trace`
 * 的条目形状。`output=null` = 未配对 ToolResult（进行中/被取消）。
 */

/** 一次工具调用轨迹。 */
export interface TraceEntry {
  id: string;
  name: string;
  args: unknown;
  output: string | null;
}
