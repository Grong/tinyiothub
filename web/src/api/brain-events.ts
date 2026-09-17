/**
 * AI 大脑事件 API（AI 大脑主干化 P0，/brain-events 域）
 *
 * 契约：后端 DTO `#[serde(rename_all = "camelCase")]`（Task 3，judgment 域先例）。
 * apiGet/apiPost 返回 ApiResponse<T>，在此解包 `.result`。
 * 列表行无 evidence——证据懒加载，展开才调 detail(id)。
 *
 * 变更操作路由（P0 不改端点）：alarm 源走 /judgments，patrol 源走
 * /workspaces/{id}/heartbeat/approvals——approve/reject 在此按 ev.source 分支。
 */

import { apiGet, apiPost, getWorkspaceId } from './client.js';
import { judgmentApi } from './judgments.js';

function unwrap<T>(res: { result: T | null }): T {
  if (res.result === null) throw new Error('空响应');
  return res.result;
}

export type BrainEventTab = 'needs_you' | 'all' | 'patrol' | 'alarm' | 'directive';

export type BrainEventSource = 'alarm' | 'patrol' | 'patrol_tick' | 'directive';

export type BrainEventStatus =
  | 'investigating'
  | 'self_closed'
  | 'awaiting_approval'
  | 'executing'
  | 'resolved'
  | 'escalated'
  | 'investigation_failed'
  | 'dismissed'
  | 'budget_skipped'
  | 'dispatch_suppressed'
  | 'patrol_ok';

/** 列表行（无 evidence——性能评审裁决：证据只由 detail 端点载）。 */
export interface BrainEvent {
  id: string;
  workspaceId: string;
  source: BrainEventSource;
  alarmId: string | null;
  thingId: string | null;
  title: string;
  verdict: string | null;
  reason: string;
  suggestedAction: string | null;
  actionCategory: string | null;
  risk: 'low' | 'medium' | 'high' | null;
  runId: string | null;
  ticketId: number | null;
  status: BrainEventStatus;
  triageMode: string;
  createdAt: string;
  judgedAt: string | null;
  stateEnteredAt: string | null;
  resolvedAt: string | null;
}

/** 详情 = 列表行 + 完整证据。 */
export interface BrainEventDetail extends BrainEvent {
  evidence: unknown;
}

/** feed 头部摘要。 */
export interface BrainEventsSummary {
  digestedToday: number;
  needsYou: number;
  latencyP50Secs: number | null;
  feedbackRight: number;
  feedbackWrong: number;
}

/** brain event id = `<source>:<原始 id>`；变更端点要原始 id（去前缀）。 */
export function brainEventTargetId(ev: BrainEvent): string {
  const idx = ev.id.indexOf(':');
  return idx >= 0 ? ev.id.slice(idx + 1) : ev.id;
}

/** patrol 源提案审批走 heartbeat 域（工作区路径参数来自会话上下文）。 */
async function postPatrolApproval(proposalId: string, action: 'approve' | 'reject'): Promise<void> {
  const ws = getWorkspaceId();
  if (!ws) throw new Error('缺少工作区上下文');
  await apiPost(
    `/workspaces/${encodeURIComponent(ws)}/heartbeat/approvals/${encodeURIComponent(proposalId)}/${action}`,
    {},
  );
}

export const brainEventApi = {
  async list(tab: BrainEventTab = 'needs_you', before?: string, limit = 20): Promise<BrainEvent[]> {
    const params: Record<string, string> = { tab, limit: String(limit) };
    if (before) params.before = before;
    const query = new URLSearchParams(params).toString();
    return unwrap(await apiGet<BrainEvent[]>(`/brain-events?${query}`));
  },

  async summary(): Promise<BrainEventsSummary> {
    return unwrap(await apiGet<BrainEventsSummary>('/brain-events/summary'));
  },

  /** 详情（含完整 evidence）——列表行无证据，展开时才调。 */
  async detail(id: string): Promise<BrainEventDetail> {
    return unwrap(await apiGet<BrainEventDetail>(`/brain-events/${encodeURIComponent(id)}`));
  },

  /** 批准：alarm 源 → /judgments/{id}/approve；patrol 源 → /heartbeat/approvals/{pid}/approve。 */
  async approve(ev: BrainEvent): Promise<void> {
    if (ev.source === 'patrol') {
      await postPatrolApproval(brainEventTargetId(ev), 'approve');
      return;
    }
    await judgmentApi.approve(brainEventTargetId(ev));
  },

  /** 拒绝：alarm 源 → /judgments/{id}/reject（必填原因，转工单）；
   *  patrol 源 → /heartbeat/approvals/{pid}/reject（端点不收原因——P0 不改）。 */
  async reject(ev: BrainEvent, reason?: string): Promise<void> {
    if (ev.source === 'patrol') {
      await postPatrolApproval(brainEventTargetId(ev), 'reject');
      return;
    }
    await judgmentApi.reject(brainEventTargetId(ev), reason ?? '');
  },
};
