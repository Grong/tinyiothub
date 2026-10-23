/**
 * E3 账本读面（spec 2026-10-21-e3 §3.1）：范围查询 + 截止时刻状态重建。
 * 只读；工作区隔离由后端保证。
 */

import { apiGet } from './client.js';

export interface LedgerEntry {
  id: number;
  ts: string;
  kind: string;
  thingId: string | null;
  payload: unknown;
}

export interface LedgerGap {
  from: string;
  to: string;
  /** retention | lagged | query_cap */
  reason: string;
}

export interface LedgerPage {
  entries: LedgerEntry[];
  nextCursor: number | null;
  hasMore: boolean;
  coverage: { from: string; to: string; gaps: LedgerGap[] };
}

export interface LedgerQuery {
  from: string;
  to: string;
  kinds?: string[];
  thingId?: string;
  cursor?: number | null;
  limit?: number;
}

export interface LedgerDeviceState {
  thingId: string;
  state: Record<string, unknown>;
  online: boolean | null;
  lastChangeAt: string;
}

export interface LedgerStateAt {
  at: string;
  devices: LedgerDeviceState[];
  precision: 'state_change';
  /** 账本无法重建的派生字段（E1 未落 health/zone 变化）。 */
  unknown: string[];
}

function unwrap<T>(res: { result: T | null; msg?: string; code: number }): T {
  if (res.result === null) throw new Error(res.msg || '空响应');
  return res.result;
}

function wsPath(workspaceId: string): string {
  return `/workspaces/${encodeURIComponent(workspaceId)}`;
}

export const ledgerApi = {
  async query(workspaceId: string, q: LedgerQuery): Promise<LedgerPage> {
    const params = new URLSearchParams();
    params.set('from', q.from);
    params.set('to', q.to);
    if (q.kinds?.length) params.set('kinds', q.kinds.join(','));
    if (q.thingId) params.set('thingId', q.thingId);
    if (q.cursor != null) params.set('cursor', String(q.cursor));
    if (q.limit != null) params.set('limit', String(q.limit));
    return unwrap(await apiGet<LedgerPage>(`${wsPath(workspaceId)}/ledger?${params.toString()}`));
  },

  async stateAt(workspaceId: string, at: string): Promise<LedgerStateAt> {
    return unwrap(
      await apiGet<LedgerStateAt>(
        `${wsPath(workspaceId)}/ledger/state?at=${encodeURIComponent(at)}`,
      ),
    );
  },
};
