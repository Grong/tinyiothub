import { describe, expect, it } from 'vitest';
import { todoProgress, toolSummary, summarizeTodos } from './timeline-render.js';

describe('timeline render helpers', () => {
  it('computes todo progress', () => {
    expect(todoProgress([])).toEqual({ done: 0, total: 0 });
    expect(
      todoProgress([
        { content: 'a', status: 'completed', activeForm: 'a' },
        { content: 'b', status: 'in_progress', activeForm: 'b' },
        { content: 'c', status: 'pending', activeForm: 'c' },
      ]),
    ).toEqual({ done: 1, total: 3 });
  });

  it('summarizes tools as name(object) -> result', () => {
    expect(
      toolSummary({
        kind: 'tool',
        id: 'c1',
        name: 'search_things',
        args: { keyword: '3号楼' },
        at: '',
      }),
    ).toBe('search_things(3号楼)');
    expect(
      toolSummary({
        kind: 'tool',
        id: 'c1',
        name: 'search_things',
        args: {},
        result: '12 台设备',
        at: '',
      }),
    ).toBe('search_things() → 12 台设备');
  });

  it('summarizes todos for the collapsed line', () => {
    expect(
      summarizeTodos([
        { content: 'a', status: 'completed', activeForm: 'a' },
        { content: 'b', status: 'in_progress', activeForm: 'b' },
      ]),
    ).toBe('计划中 1/2');
  });
});
