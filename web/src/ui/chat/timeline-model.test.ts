import { describe, expect, it } from 'vitest';
import { TimelineModel } from './timeline-model.js';
import type { SessionEvent } from '../../api/assistant.js';

function event(seq: number, eventType: string, payloadJson: Record<string, unknown>): SessionEvent {
  return {
    seq,
    eventId: `e${seq}`,
    sessionId: 'agent:ws:a/s1',
    executionMode: 'production',
    runId: 'run1',
    taskId: null,
    eventType,
    occurredAt: '2026-11-07T00:00:00Z',
    causeId: null,
    schemaV: 1,
    payloadDigest: 'd',
    payloadJson,
  };
}

describe('TimelineModel', () => {
  it('dedupes by eventId and keeps seq order', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(2, 'message.created', { messageId: 'm2', role: 'user', text: '你好' }));
    model.ingestEvent(event(1, 'turn.started', { turnId: 't1' }));
    model.ingestEvent(event(2, 'message.created', { messageId: 'm2', role: 'user', text: '你好' }));
    expect(model.blocks.map((block) => block.kind)).toEqual(['turn', 'user']);
    expect(model.cursor).toBe(2);
    expect(model.firstUserText).toBe('你好');
  });

  it('pairs tools by toolCallId and records evidence', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'tool.called', { toolCallId: 'c1', tool: 'search_things', args: { q: 'x' } }));
    model.ingestEvent(event(2, 'tool.returned', { toolCallId: 'c1', tool: 'search_things', result: 'ok', evidence: [{ evidenceId: 'ev1' }] }));
    const tool = model.blocks.find((block) => block.kind === 'tool');
    expect(tool).toMatchObject({ id: 'c1', name: 'search_things', result: 'ok' });
    expect((tool as any).evidence).toHaveLength(1);
  });

  it('replaces todos from todo.updated', () => {
    const model = new TimelineModel();
    const first = [{ content: '检查离线', status: 'in_progress', activeForm: '检查离线' }];
    const second = [
      { content: '检查离线', status: 'completed', activeForm: '检查离线' },
      { content: '生成摘要', status: 'pending', activeForm: '生成摘要' },
    ];
    model.ingestEvent(event(1, 'todo.updated', { todos: first }));
    model.ingestEvent(event(2, 'todo.updated', { todos: second }));
    expect(model.todos).toHaveLength(2);
    expect(model.todos[0].status).toBe('completed');
  });

  it('tracks asked/answered questions', () => {
    const model = new TimelineModel();
    const questions = [{ question: '范围？', header: '范围', options: [{ label: '全部', description: '' }], multiSelect: false }];
    model.ingestEvent(event(1, 'question.asked', { questionId: 'q1', questions }));
    expect(model.pendingQuestion?.questionId).toBe('q1');
    model.ingestEvent(event(2, 'question.answered', { questionId: 'q1', answers: [{ questionIndex: 0, selected: ['全部'] }], timedOut: false }));
    expect(model.pendingQuestion).toBeNull();
    const block = model.blocks.find((block) => block.kind === 'notice');
    expect(block).toMatchObject({ tone: 'answered' });
  });

  it('tracks approval lifecycle', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'approval.requested', { approvalId: 'a1', action: 'set_device', summary: '设 24℃' }));
    expect(model.pendingApproval?.approvalId).toBe('a1');
    model.ingestEvent(event(2, 'approval.resolved', { approvalId: 'a1', resolution: 'approved' }));
    expect(model.pendingApproval).toBeNull();
  });

  it('updates subagent lifecycle and turn status', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'subagent.started', { subagentId: 's1', description: '分析离线', subagentType: 'general-purpose' }));
    model.ingestEvent(event(2, 'subagent.completed', { subagentId: 's1', status: 'completed', summary: '2 台 unknown' }));
    const subagent = model.blocks.find((block) => block.kind === 'subagent');
    expect(subagent).toMatchObject({ id: 's1', status: 'completed', summary: '2 台 unknown' });

    model.ingestEvent(event(3, 'turn.started', { turnId: 't1' }));
    model.ingestEvent(event(4, 'turn.completed', { turnId: 't1', outcome: 'completed', usage: null }));
    const turn = model.blocks.find((block) => block.kind === 'turn');
    expect(turn).toMatchObject({ status: 'completed' });
  });
});
