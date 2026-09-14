/**
 * 处置判断 API（AI 大脑主干化 P0，/judgments 域）
 *
 * 契约：后端 DTO `#[serde(rename_all = "camelCase")]` 源头驼峰（ticket 域先例）。
 * apiGet/apiPost 返回 ApiResponse<T>，在此解包 `.result`。
 */

import { apiGet, apiPost } from './client.js';

function unwrap<T>(res: { result: T | null }): T {
  if (res.result === null) throw new Error('空响应');
  return res.result;
}

export interface JudgmentFeedback {
  verdict: 'right' | 'wrong';
  reason: string | null;
  createdAt: string;
}

export interface Judgment {
  id: string;
  alarmId: string | null;
  thingId: string | null;
  ticketId: number | null;
  verdict: 'noise' | 'self_healable' | 'needs_human' | null;
  reason: string;
  evidence: unknown;
  suggestedAction: string | null;
  actionCategory: string | null;
  status:
    | 'investigating'
    | 'noise_archived'
    | 'awaiting_approval'
    | 'executing'
    | 'resolved'
    | 'escalated'
    | 'investigation_failed'
    | 'budget_skipped'
    | 'dispatch_suppressed';
  latestFeedback: JudgmentFeedback | null;
  createdAt: string;
  judgedAt: string | null;
  resolvedAt: string | null;
}

export interface JudgmentSummary {
  needsYou: number;
  investigating: number;
  digestedToday: number;
  feedbackTotal: number;
  feedbackRight: number;
  feedbackWrong: number;
  /** E2：判断延迟分位数（秒）；验收「5 分钟出判断 ≥90%」的度量 */
  latencyP50Secs: number | null;
  latencyP90Secs: number | null;
  /** E5：按动作类别的最新反馈对错数（P1 转正决策度量） */
  feedbackByCategory: Record<string, { right: number; wrong: number }>;
  /** F11：审批超时小时数（后端 cron 配置下发，倒计时同源） */
  approvalTimeoutHours: number;
}

export type JudgmentTab = 'needs_you' | 'investigating' | 'resolved' | 'noise' | 'all';

export const judgmentApi = {
  async list(tab: JudgmentTab = 'needs_you', before?: string, pageSize = 20): Promise<Judgment[]> {
    const params: Record<string, string> = { tab, page_size: String(pageSize) };
    if (before) params.before = before;
    const query = new URLSearchParams(params).toString();
    return unwrap(await apiGet<Judgment[]>(`/judgments?${query}`));
  },

  async summary(): Promise<JudgmentSummary> {
    return unwrap(await apiGet<JudgmentSummary>('/judgments/summary'));
  },

  async feedback(id: string, verdict: 'right' | 'wrong', reason?: string): Promise<{ ok: boolean }> {
    return unwrap(await apiPost<{ ok: boolean }>(`/judgments/${id}/feedback`, { verdict, reason }));
  },

  async approve(id: string): Promise<{ ok: boolean; status: string }> {
    return unwrap(await apiPost<{ ok: boolean; status: string }>(`/judgments/${id}/approve`, {}));
  },

  async reject(id: string, reason: string): Promise<{ ok: boolean; status: string; ticketId: number | null }> {
    return unwrap(
      await apiPost<{ ok: boolean; status: string; ticketId: number | null }>(`/judgments/${id}/reject`, { reason }),
    );
  },
};
