import { render } from 'lit';
import { describe, expect, it } from 'vitest';
import { renderTimeline } from './timeline-render.js';
import type { ContentPart } from './timeline-model.js';

const at = '2026-11-11T17:00:00Z';

describe('process detail surfaces', () => {
  it('uses one content container for reasoning, tools, subagents, and tasks', () => {
    const parts: ContentPart[] = [
      { kind: 'turn', id: 't1', status: 'running', at },
      { kind: 'reasoning', id: 'r1', text: '分析当前状态', turnId: 't1', streaming: true, at },
      { kind: 'tool', id: 'c1', name: 'read_device', args: {}, result: 'online', at },
      { kind: 'subagent', id: 's1', description: '检查告警', summary: '没有高优先级告警', at },
    ];
    const host = document.createElement('div');
    render(
      renderTimeline(parts, {
        todos: [{ content: '检查设备', status: 'in_progress', activeForm: '检查设备' }],
      }),
      host,
    );
    expect(host.querySelectorAll('.turn-process .process-detail')).toHaveLength(4);
    expect(host.querySelector('.timeline-todos')?.classList.contains('process-detail')).toBe(true);
  });
});
