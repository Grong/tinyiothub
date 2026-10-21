import { describe, expect, it } from 'vitest';
import type { LedgerEntry } from '../../api/ledger.js';
import { buildFlow, chipPosition } from './flow-model.js';

function entry(
  id: number,
  kind: string,
  thingId: string | null,
  payload: Record<string, unknown>,
): LedgerEntry {
  return { id, ts: `2026-10-21T10:00:${String(id).padStart(2, '0')}Z`, kind, thingId, payload };
}

describe('buildFlow', () => {
  it('groups device lanes and merges composite chain into one lane', () => {
    const entries = [
      entry(1, 'state_change', 't1', { field: '温度', new: '31' }),
      entry(2, 'watch_hit', 't1', { matched: true, rule_id: 'wr_1' }),
      entry(3, 'job', 't1', { compositeId: 'guard', stepIndex: 0, status: 'accepted' }),
      entry(4, 'job', 't2', { compositeId: 'guard', stepIndex: 1, status: 'result' }),
      entry(5, 'state_change', 't2', { field: '状态', new: 'closed' }),
    ];
    const model = buildFlow(entries);
    expect(model.chains).toHaveLength(1);
    expect(model.chains[0]).toMatchObject({ id: 'guard', startId: 3, endId: 4 });
    const chainLane = model.lanes.find((l) => l.chain);
    expect(chainLane?.chips.map((c) => c.id)).toEqual([3, 4]);
    expect(model.lanes.some((l) => l.key === 'thing:t1')).toBe(true);
    expect(model.lanes.some((l) => l.key === 'thing:t2')).toBe(true);
  });

  it('sorts chips by ledger id and skips unknown kinds', () => {
    const model = buildFlow([
      entry(9, 'state_change', 't1', { field: 'a', new: '1' }),
      entry(7, 'state_change', 't1', { field: 'a', new: '0' }),
      entry(8, 'rate_limited', 't1', {}),
    ]);
    expect(model.lanes[0].chips.map((c) => c.id)).toEqual([7, 9]);
  });

  it('passes gaps through and maps watch misses', () => {
    const model = buildFlow(
      [entry(1, 'watch_timeout', 't1', { matched: false, rule_id: 'wr_2' })],
      [{ from: 'a', to: 'b', reason: 'query_cap' }],
    );
    expect(model.gaps).toHaveLength(1);
    expect(model.lanes[0].chips[0].status).toBe('missed');
    expect(model.lanes[0].chips[0].label).toContain('watch 未命中');
  });
});

describe('chipPosition', () => {
  it('clamps into 0..100 and handles a single point', () => {
    expect(chipPosition(5, 5, 5)).toBe(0);
    expect(chipPosition(10, 0, 20)).toBe(50);
    expect(chipPosition(-1, 0, 20)).toBe(0);
  });
});
