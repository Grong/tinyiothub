/**
 * 工单 API（工单模块 M1）
 */

import { apiGet, apiPost } from './client.js';

export interface TicketStep {
  action: string;
  result: string;
  error?: string | null;
}

export interface TicketBriefing {
  problem?: string;
  steps_attempted?: TicketStep[];
  last_error?: string | null;
  failure_kind?: string | null;
  suggested_next_steps?: string[];
}

export interface Ticket {
  id: number;
  workspace_id: string;
  thing_id: string | null;
  agent_run_id: string;
  title: string;
  briefing: TicketBriefing;
  state: 'open' | 'claimed' | 'in_progress' | 'resolved' | 'closed';
  assignee_id: string | null;
  resolution_text: string | null;
  created_at: string;
  claimed_at: string | null;
  resolved_at: string | null;
  closed_at: string | null;
}

export interface TicketEvent {
  id: number;
  kind: 'state_change' | 'system';
  actor_type: 'user' | 'agent' | 'system';
  actor_id: string | null;
  payload: {
    from?: string;
    to?: string;
    kind?: string;
    count?: number;
    agent_run_id?: string;
  } | null;
  created_at: string;
}

export interface TicketDetail extends Ticket {
  events: TicketEvent[];
}

export interface TicketListPayload {
  tickets: Ticket[];
  unclaimed_count: number;
}

export interface TicketStatistics {
  tickets_total: number;
  runs_total: number;
  escalation_rate: number;
  open: number;
  claimed: number;
  in_progress: number;
  resolved: number;
  closed: number;
  avg_time_to_ack_secs: number | null;
  avg_time_to_resolve_secs: number | null;
}

export const ticketApi = {
  async list(params?: { state?: string; page?: number; page_size?: number }) {
    return apiGet<TicketListPayload>('/tickets', params as Record<string, any>);
  },

  async detail(id: number) {
    return apiGet<TicketDetail>(`/tickets/${id}`);
  },

  async claim(id: number) {
    return apiPost<void>(`/tickets/${id}/claim`);
  },

  async start(id: number) {
    return apiPost<void>(`/tickets/${id}/start`);
  },

  async resolve(id: number, resolutionText: string) {
    return apiPost<void>(`/tickets/${id}/resolve`, { resolution_text: resolutionText });
  },

  async close(id: number) {
    return apiPost<void>(`/tickets/${id}/close`);
  },

  async abandon(id: number) {
    return apiPost<void>(`/tickets/${id}/abandon`);
  },

  async reopen(id: number) {
    return apiPost<void>(`/tickets/${id}/reopen`);
  },

  async statistics() {
    return apiGet<TicketStatistics>('/tickets/statistics');
  },
};
