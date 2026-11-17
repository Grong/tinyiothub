import { render } from 'lit';
import { describe, expect, it } from 'vitest';
import { renderTimeline, todoProgress, toolSummary, summarizeTodos } from './timeline-render.js';
import type { ContentPart } from './timeline-model.js';

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
    ).toBe('查找设备 3号楼');
    expect(
      toolSummary({
        kind: 'tool',
        id: 'c1',
        name: 'search_things',
        args: {},
        result: '12 台设备',
        at: '',
      }),
    ).toBe('查找设备 → 12 台设备');
  });

  it('summarizes known device JSON instead of exposing raw JSON', () => {
    expect(
      toolSummary({
        kind: 'tool',
        id: 'c1',
        name: 'read_device',
        args: { thingId: 'dev-1' },
        result: JSON.stringify({ name: '3号楼传感器', online: true, state: { 温度: 24, 湿度: 60 } }),
        at: '',
      }),
    ).toBe('查看设备 dev-1 → 在线，2 项属性');
  });

  it('uses the structured error for a failed tool summary', () => {
    expect(
      toolSummary({
        kind: 'tool',
        id: 'c1',
        name: 'get_skill',
        args: { skill_name: 'patrol' },
        result: '{"error":"Skill patrol not found","success":false}',
        status: 'failed',
        error: 'Skill patrol not found',
        at: '',
      }),
    ).toBe('读取技能 patrol → Skill patrol not found');
  });

  it('summarizes todos for the collapsed line', () => {
    expect(
      summarizeTodos([
        { content: 'a', status: 'completed', activeForm: 'a' },
        { content: 'b', status: 'in_progress', activeForm: 'b' },
      ]),
    ).toBe('计划中 1/2');
  });

  it('places the plan inside the latest turn only', () => {
    const parts: ContentPart[] = [
      {
        kind: 'turn',
        id: 'old',
        status: 'completed',
        at: '2026-11-09T10:00:00Z',
        completedAt: '2026-11-09T10:00:02Z',
      },
      {
        kind: 'assistant',
        id: 'old-answer',
        text: '上一轮结论',
        at: '2026-11-09T10:00:01Z',
        turnId: 'old',
      },
      { kind: 'turn', id: 'current', status: 'running', at: '2026-11-09T10:01:00Z' },
      {
        kind: 'tool',
        id: 'call-1',
        name: 'search_things',
        args: { keyword: '3号楼' },
        at: '2026-11-09T10:01:01Z',
      },
    ];
    const host = document.createElement('div');
    render(
      renderTimeline(parts, {
        sending: true,
        todos: [
          { content: '检查离线', status: 'completed', activeForm: '检查离线' },
          { content: '生成摘要', status: 'in_progress', activeForm: '生成摘要' },
        ],
      }),
      host,
    );

    const groups = host.querySelectorAll('.timeline-turn-group');
    expect(groups).toHaveLength(2);
    expect(groups[0].querySelector('.timeline-todos')).toBeNull();
    expect(groups[1].querySelector('.timeline-todos__count')?.textContent).toBe('1/2');
    expect(groups[1].querySelector('.timeline-todo--completed')?.textContent).toContain('检查离线');
    expect(groups[1].querySelector('.timeline-todo--in_progress')?.textContent).toContain(
      '生成摘要',
    );
  });

  it('derives running and completion status from the latest turn', () => {
    const parts: ContentPart[] = [
      { kind: 'turn', id: 'current', status: 'running', at: '2026-11-09T10:00:00Z' },
      {
        kind: 'tool',
        id: 'call-1',
        name: 'search_things',
        args: { keyword: '3号楼' },
        at: '2026-11-09T10:00:01Z',
      },
    ];
    const host = document.createElement('div');
    render(renderTimeline(parts), host);

    const current = () => host.querySelector('.timeline-turn-group--current');
    expect(current()).not.toBeNull();
    expect(current()?.querySelector('.turn-footer')?.textContent).toContain('查找设备 3号楼');

    parts[0] = {
      kind: 'turn',
      id: 'current',
      status: 'completed',
      at: '2026-11-09T10:00:00Z',
      completedAt: '2026-11-09T10:00:03Z',
    };
    render(renderTimeline(parts), host);

    expect(current()?.querySelector('.turn-footer')?.textContent).toContain('已完成');
    expect(current()?.querySelector('.turn-footer')?.textContent).toContain('3.0s');
  });

  it('renders retry reason, attempt, and delay', () => {
    const parts: ContentPart[] = [
      { kind: 'turn', id: 't1', status: 'running', at: '2026-11-09T10:00:00Z' },
      {
        kind: 'retry',
        id: 'retry-1',
        turnId: 't1',
        attempt: 1,
        maxRetries: 3,
        delayMs: 1000,
        error: 'HTTP 503: unavailable',
        at: '2026-11-09T10:00:01Z',
      },
    ];
    const host = document.createElement('div');
    render(renderTimeline(parts), host);
    const retry = host.querySelector('.timeline-retry');
    expect(retry?.textContent).toContain('HTTP 503: unavailable');
    expect(retry?.textContent).toContain('1/3');
    expect(retry?.textContent).toContain('1 秒');
  });
});
