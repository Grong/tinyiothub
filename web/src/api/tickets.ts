/**
 * 工单 API（工单模块）
 *
 * 契约（CI tsc 失败根因修复）：后端 DTO `#[serde(rename_all = "camelCase")]`
 * 源头驼峰（alarm 域先例）；本文件类型一律 camelCase，与 client.ts 的
 * keysToCamelCase（运行时全局）/ KeysToCamelCase（类型级仅首下划线）
 * 两个转换器都幂等相遇。apiGet/apiPost 返回 ApiResponse<T>，在此解包
 * `.result`——调用方拿裸数据。
 */

import { apiGet, apiPost } from './client.js';

function unwrap<T>(res: { result: T | null }): T {
  if (res.result === null) throw new Error('空响应');
  return res.result;
}

export interface TicketStep {
  action: string;
  result: string;
  error?: string | null;
}

/** 简报（subscriber 写入 DB 为 snake_case JSON，经运行时 keysToCamelCase 全局转换后为驼峰）。 */
export interface TicketBriefing {
  problem?: string;
  stepsAttempted?: TicketStep[];
  lastError?: string | null;
  failureKind?: string | null;
  suggestedNextSteps?: string[];
}

export interface Ticket {
  id: number;
  workspaceId: string;
  thingId: string | null;
  agentRunId: string;
  sessionKey: string | null;
  title: string;
  briefing: TicketBriefing;
  state: 'open' | 'claimed' | 'in_progress' | 'resolved' | 'closed';
  assigneeId: string | null;
  resolutionText: string | null;
  createdAt: string;
  claimedAt: string | null;
  resolvedAt: string | null;
  closedAt: string | null;
}

export interface TicketEvent {
  id: number;
  kind: 'state_change' | 'system';
  actorType: 'user' | 'agent' | 'system';
  actorId: string | null;
  payload: {
    from?: string;
    to?: string;
    kind?: string;
    count?: number;
    agentRunId?: string;
    priorResolution?: string;
  } | null;
  createdAt: string;
}

export interface TicketDetail extends Ticket {
  events: TicketEvent[];
}

export interface TicketListPayload {
  tickets: Ticket[];
  unclaimedCount: number;
}

export interface TicketStatistics {
  ticketsTotal: number;
  runsTotal: number;
  escalationRate: number;
  open: number;
  claimed: number;
  inProgress: number;
  resolved: number;
  closed: number;
  avgTimeToAckSecs: number | null;
  avgTimeToResolveSecs: number | null;
}

export const ticketApi = {
  async list(params?: { state?: string; page?: number; page_size?: number }): Promise<TicketListPayload> {
    return unwrap(await apiGet<TicketListPayload>('/tickets', params as Record<string, any>));
  },

  async detail(id: number): Promise<TicketDetail> {
    return unwrap(await apiGet<TicketDetail>(`/tickets/${id}`));
  },

  async claim(id: number): Promise<void> {
    await apiPost<void>(`/tickets/${id}/claim`);
  },

  async start(id: number): Promise<void> {
    await apiPost<void>(`/tickets/${id}/start`);
  },

  async resolve(id: number, resolutionText: string): Promise<void> {
    await apiPost<void>(`/tickets/${id}/resolve`, { resolutionText });
  },

  async close(id: number): Promise<void> {
    await apiPost<void>(`/tickets/${id}/close`);
  },

  async abandon(id: number): Promise<void> {
    await apiPost<void>(`/tickets/${id}/abandon`);
  },

  async reopen(id: number): Promise<void> {
    await apiPost<void>(`/tickets/${id}/reopen`);
  },

  async statistics(): Promise<TicketStatistics> {
    return unwrap(await apiGet<TicketStatistics>('/tickets/statistics'));
  },
};
