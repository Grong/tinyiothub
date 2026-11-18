import { render } from 'lit';
import { describe, expect, it } from 'vitest';
import { renderTimeline } from './timeline-render.js';
import type { ContentPart } from './timeline-model.js';

function tool(id: string, name = 'read_device', status?: 'ok' | 'failed'): ContentPart {
  return {
    kind: 'tool',
    id,
    name,
    args: {},
    result: status === 'failed' ? undefined : 'ok',
    status,
    error: status === 'failed' ? 'boom' : undefined,
    at: '2026-11-11T15:00:00Z',
  };
}

describe('consecutive tool presentation', () => {
  it('keeps adjacent tools as independent expandable items with visible status', () => {
    const host = document.createElement('div');
    render(
      renderTimeline([tool('c1'), tool('c2', 'list_alarms'), tool('c3', 'query_events')]),
      host,
    );
    expect(host.querySelectorAll('.timeline-tools')).toHaveLength(0);
    expect(host.querySelectorAll('.turn-process details.activity--tool')).toHaveLength(3);
    expect(host.querySelectorAll('.turn-process .activity__meta--ok')).toHaveLength(3);
  });

  it('preserves failure status without adding a tool grouping layer', () => {
    const host = document.createElement('div');
    const assistant: ContentPart = {
      kind: 'assistant',
      id: 'a1',
      text: '中间结论',
      at: '2026-11-11T15:00:01Z',
    };
    render(
      renderTimeline([tool('c1'), tool('c2'), assistant, tool('c3', 'read_device', 'failed')]),
      host,
    );
    expect(host.querySelectorAll('.timeline-tools')).toHaveLength(0);
    expect(host.querySelector('.turn-process .activity__meta--failed')?.textContent).toBe('失败');
  });
});
