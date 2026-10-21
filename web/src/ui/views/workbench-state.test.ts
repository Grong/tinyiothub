import { describe, expect, it } from 'vitest';
import {
  backToNow,
  isHistorical,
  parseWorkbenchUrl,
  toWorkbenchUrl,
} from './workbench-state.js';

describe('workbench URL state', () => {
  it('defaults to devices tab, live mode', () => {
    const s = parseWorkbenchUrl('');
    expect(s).toEqual({ tab: 'devices', at: null, event: null, device: null });
    expect(isHistorical(s)).toBe(false);
  });

  it('parses deep link and round-trips', () => {
    const s = parseWorkbenchUrl('?tab=flow&at=2026-10-21T14%3A32%3A07Z&event=42&device=t1');
    expect(s.tab).toBe('flow');
    expect(s.event).toBe(42);
    expect(s.device).toBe('t1');
    expect(isHistorical(s)).toBe(true);
    expect(parseWorkbenchUrl(toWorkbenchUrl(s))).toEqual(s);
  });

  it('ignores invalid tab and event', () => {
    const s = parseWorkbenchUrl('?tab=nope&event=abc');
    expect(s.tab).toBe('devices');
    expect(s.event).toBeNull();
  });

  it('back to now clears history anchors but keeps flow/world tab', () => {
    expect(backToNow({ tab: 'world', at: 'x', event: 1, device: 't' })).toEqual({
      tab: 'world',
      at: null,
      event: null,
      device: null,
    });
    expect(backToNow({ tab: 'devices', at: 'x', event: 1, device: 't' }).tab).toBe('flow');
  });
});
