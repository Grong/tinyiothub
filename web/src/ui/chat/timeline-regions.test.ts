import { html, render } from 'lit';
import { describe, expect, it } from 'vitest';
import { renderTimeline } from './timeline-render.js';
import type { ContentPart } from './timeline-model.js';

const at = '2026-11-11T16:00:00Z';
const doneAt = '2026-11-11T16:01:15Z';

function turn(status: 'running' | 'completed' | 'failed'): ContentPart {
  return {
    kind: 'turn',
    id: 't1',
    status,
    at,
    completedAt: status === 'running' ? undefined : doneAt,
    usage: { inputTokens: 2000, outputTokens: 1200 },
  };
}

function tool(id: string): ContentPart {
  return { kind: 'tool', id, name: 'read_device', args: { thingId: id }, result: 'ok', at };
}

describe('turn functional regions', () => {
  it('renders one header, process region, body, and footer with real usage', () => {
    const host = document.createElement('div');
    const answer: ContentPart = { kind: 'assistant', id: 'a1', text: '本轮结论', at, turnId: 't1' };
    render(renderTimeline([turn('running'), tool('c1'), tool('c2'), answer]), host);
    expect(host.querySelectorAll('.turn-header')).toHaveLength(1);
    expect(host.querySelectorAll('.turn-process')).toHaveLength(1);
    expect(host.querySelectorAll('.timeline-assistant')).toHaveLength(1);
    expect(host.querySelectorAll('.turn-footer')).toHaveLength(1);
    expect(host.querySelector('.turn-header')?.textContent).toContain('3.2k tokens');
    expect(host.querySelector('.turn-process__summary')?.textContent).toContain('2 个工具');
  });

  it('keeps process collapsed by default and expands it on demand', () => {
    const host = document.createElement('div');
    render(renderTimeline([turn('running'), tool('c1')]), host);
    const process = host.querySelector('.turn-process');
    expect(process?.hasAttribute('open')).toBe(false);
    process?.querySelector('summary')?.click();
    expect(process?.hasAttribute('open')).toBe(true);
  });

  it('keeps tasks and independent tools inside the expandable process region', () => {
    const host = document.createElement('div');
    const skill: ContentPart = { kind: 'skill', id: 's1', name: 'inspection', version: '1.0', at };
    render(
      renderTimeline([turn('running'), skill, tool('c1'), tool('c2')], {
        todos: [{ content: '检查设备', status: 'in_progress', activeForm: '检查设备' }],
      }),
      host,
    );
    expect(host.querySelector('.turn-process .timeline-todos')).not.toBeNull();
    expect(host.querySelectorAll('.turn-process details.activity--tool')).toHaveLength(2);
    expect(host.querySelectorAll('.turn-process .activity__meta--ok')).toHaveLength(2);
  });

  it('shows the latest process item in the collapsed summary and updates it', () => {
    const host = document.createElement('div');
    render(renderTimeline([turn('running'), tool('c1')]), host);
    expect(host.querySelector('.turn-process__latest')?.textContent).toContain('查看设备');
    render(renderTimeline([turn('running'), tool('c1'), tool('c2')]), host);
    const latest = host.querySelector('.turn-process__latest');
    expect(latest?.textContent).toContain('查看设备');
    expect(latest?.getAttribute('title')).toContain('c2');
  });

  it('uses waiting-for-approval as the primary state and keeps actions in the current turn footer', () => {
    const host = document.createElement('div');
    render(
      renderTimeline([turn('running'), tool('c1')], {
        waitingForUser: true,
        turnActions: html`<button>批准</button>`,
      }),
      host,
    );
    expect(host.querySelector('.turn-header')?.textContent).toContain('等待批准');
    expect(host.querySelector('.turn-footer')?.textContent).toContain('等待批准');
    expect(host.querySelector('.turn-footer')?.textContent).not.toContain('等待模型响应');
    expect(host.querySelector('.turn-footer .turn-actions')?.textContent).toContain('批准');
  });

  it('keeps one collapsed process region when body segments interleave', () => {
    const host = document.createElement('div');
    const answer: ContentPart = { kind: 'assistant', id: 'a1', text: '中间发现', at, turnId: 't1' };
    const reasoning: ContentPart = {
      kind: 'reasoning',
      id: 'r1',
      text: '继续核对',
      turnId: 't1',
      streaming: true,
      at,
    };
    render(renderTimeline([turn('running'), tool('c1'), answer, tool('c2'), reasoning]), host);
    expect(host.querySelectorAll('.turn-process')).toHaveLength(1);
    expect(host.querySelector('.turn-process')?.hasAttribute('open')).toBe(false);
    expect(host.querySelectorAll('.turn-process details.activity--tool')).toHaveLength(2);
  });

  it('shows only one assistant identity in a turn with persisted or live body text', () => {
    const host = document.createElement('div');
    const answer: ContentPart = { kind: 'assistant', id: 'a1', text: '正文', at, turnId: 't1' };
    render(renderTimeline([turn('running'), answer], { liveText: '正在输出' }), host);
    const header = host.querySelector('.timeline-turn-group--current');
    expect(header?.querySelectorAll('.turn-header')).toHaveLength(1);
    expect(header?.querySelector('.assistant-message__head')).toBeNull();
  });

  it('places terminal error details in the footer', () => {
    const host = document.createElement('div');
    const failed = { ...turn('failed'), error: 'HTTP 503: unavailable' } as ContentPart;
    render(renderTimeline([failed]), host);
    expect(host.querySelector('.turn-footer__error')?.textContent).toContain(
      'HTTP 503: unavailable',
    );
  });
});
