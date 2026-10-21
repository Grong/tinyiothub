/**
 * E3 Flow 纯函数模型（spec 2026-10-21-e3 §3.4）：
 * 账本条目 → 泳道/事件 chip/协同链括号/缺口。无 DOM，可单测。
 */

import type { LedgerEntry, LedgerGap } from '../../api/ledger.js';

export type ChipKind = 'state_change' | 'watch' | 'job';

export interface FlowChip {
  id: number;
  ts: string;
  kind: ChipKind;
  label: string;
  thingId: string | null;
  status: string | null;
  /** 协同链事件携带的 compositeId。 */
  chainId: string | null;
  entry: LedgerEntry;
}

export interface FlowLane {
  key: string;
  label: string;
  chips: FlowChip[];
  /** true = 协同链泳道（固定排在设备泳道前）。 */
  chain: boolean;
}

export interface FlowChain {
  id: string;
  label: string;
  /** 以账本 id 作为横向位置。 */
  startId: number;
  endId: number;
}

export interface FlowModel {
  lanes: FlowLane[];
  chains: FlowChain[];
  gaps: LedgerGap[];
}

function payloadOf(entry: LedgerEntry): Record<string, unknown> {
  return entry.payload && typeof entry.payload === 'object'
    ? (entry.payload as Record<string, unknown>)
    : {};
}

function chipOf(entry: LedgerEntry): FlowChip | null {
  const payload = payloadOf(entry);
  const chainId =
    typeof payload.compositeId === 'string' ? payload.compositeId : null;
  if (entry.kind === 'state_change') {
    const field = String(payload.field ?? '?');
    const value = payload.new;
    return {
      id: entry.id,
      ts: entry.ts,
      kind: 'state_change',
      label: `${field}→${value == null ? '?' : String(value)}`,
      thingId: entry.thingId,
      status: null,
      chainId,
      entry,
    };
  }
  if (entry.kind.startsWith('watch')) {
    const matched = payload.matched !== false;
    return {
      id: entry.id,
      ts: entry.ts,
      kind: 'watch',
      label: `${matched ? 'watch 命中' : 'watch 未命中'}${
        payload.rule_id ? ` · ${String(payload.rule_id)}` : ''
      }`,
      thingId: entry.thingId,
      status: matched ? 'matched' : 'missed',
      chainId,
      entry,
    };
  }
  if (entry.kind === 'job') {
    const status = typeof payload.status === 'string' ? payload.status : 'job';
    return {
      id: entry.id,
      ts: entry.ts,
      kind: 'job',
      label: `job ${status}${payload.stepIndex != null ? ` #${String(payload.stepIndex)}` : ''}`,
      thingId: entry.thingId,
      status,
      chainId,
      entry,
    };
  }
  return null;
}

export function buildFlow(entries: LedgerEntry[], gaps: LedgerGap[] = []): FlowModel {
  const lanes = new Map<string, FlowLane>();
  const chains = new Map<string, FlowChain>();

  for (const entry of entries) {
    const chip = chipOf(entry);
    if (!chip) continue;
    const key = chip.chainId ? `chain:${chip.chainId}` : `thing:${entry.thingId ?? 'workspace'}`;
    const label = chip.chainId
      ? `协同链 · ${chip.chainId}`
      : entry.thingId ?? '工作区';
    const lane =
      lanes.get(key) ?? { key, label, chips: [], chain: Boolean(chip.chainId) };
    lane.chips.push(chip);
    lanes.set(key, lane);

    if (chip.chainId) {
      const existing = chains.get(chip.chainId);
      if (existing) {
        existing.startId = Math.min(existing.startId, chip.id);
        existing.endId = Math.max(existing.endId, chip.id);
      } else {
        chains.set(chip.chainId, {
          id: chip.chainId,
          label: `协同链 · ${chip.chainId}`,
          startId: chip.id,
          endId: chip.id,
        });
      }
    }
  }

  const sorted = [...lanes.values()]
    .map((lane) => ({
      ...lane,
      chips: lane.chips.sort((a, b) => a.id - b.id),
    }))
    .sort((a, b) => {
      if (a.chain !== b.chain) return a.chain ? -1 : 1;
      return a.label.localeCompare(b.label, 'zh-CN');
    });

  return {
    lanes: sorted,
    chains: [...chains.values()].sort((a, b) => a.startId - b.startId),
    gaps,
  };
}

/** 选中 chip 在时间轴上的百分比位置（按 id 线性投影）。 */
export function chipPosition(chipId: number, minId: number, maxId: number): number {
  if (maxId <= minId) return 0;
  return Math.max(0, Math.min(100, ((chipId - minId) / (maxId - minId)) * 100));
}

/** 缺口在时间轴上的区间（按账本 id 无法映射时返回 null，由 UI 降级为横幅）。 */
export function gapRange(
  gap: LedgerGap,
  entries: LedgerEntry[],
): { startId: number; endId: number } | null {
  if (entries.length === 0) return null;
  const startId = entries[0].id;
  const endId = entries[entries.length - 1].id;
  return gap.reason === 'query_cap' ? { startId, endId } : null;
}
