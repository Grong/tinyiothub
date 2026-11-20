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
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(event(2, 'message.created', { messageId: 'm2', role: 'user', text: '你好' }));
    expect(model.blocks.map((block) => block.kind)).toEqual(['turn', 'user']);
    expect(model.cursor).toBe(2);
    expect(model.firstUserText).toBe('你好');
  });

  it('pairs tools by toolCallId and records evidence', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'tool/call', { toolCallId: 'c1', tool: 'search_things', args: { q: 'x' } }),
    );
    model.ingestEvent(
      event(2, 'tool/result', {
        toolCallId: 'c1',
        tool: 'search_things',
        result: 'ok',
        evidence: [{ evidenceId: 'ev1' }],
      }),
    );
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
    const questions = [
      {
        question: '范围？',
        header: '范围',
        options: [{ label: '全部', description: '' }],
        multiSelect: false,
      },
    ];
    model.ingestEvent(event(1, 'question.asked', { questionId: 'q1', questions }));
    expect(model.pendingQuestion?.questionId).toBe('q1');
    model.ingestEvent(
      event(2, 'question.answered', {
        questionId: 'q1',
        answers: [{ questionIndex: 0, selected: ['全部'] }],
        timedOut: false,
      }),
    );
    expect(model.pendingQuestion).toBeNull();
    const block = model.blocks.find((block) => block.kind === 'notice');
    expect(block).toMatchObject({ tone: 'answered' });
  });

  it('tracks approval lifecycle', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'approval.requested', { approvalId: 'a1', action: 'set_device', summary: '设 24℃' }),
    );
    expect(model.pendingApprovals.map((pending) => pending.approvalId)).toEqual(['a1']);
    model.ingestEvent(event(2, 'approval.resolved', { approvalId: 'a1', resolution: 'approved' }));
    expect(model.pendingApprovals).toEqual([]);
  });

  it('tracks multiple pending approvals independently', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'approval.requested', { approvalId: 'a1', action: 'set_device', summary: '设 24℃' }),
    );
    model.ingestEvent(
      event(2, 'approval.requested', {
        approvalId: 'a2',
        action: 'call_device',
        summary: '打开开关',
      }),
    );
    model.ingestEvent(
      event(3, 'approval.requested', { approvalId: 'a1', action: 'set_device', summary: '设 24℃' }),
    );
    expect(model.pendingApprovals.map((pending) => pending.approvalId)).toEqual(['a1', 'a2']);

    model.ingestEvent(event(4, 'approval.resolved', { approvalId: 'a1', resolution: 'approved' }));
    expect(model.pendingApprovals.map((pending) => pending.approvalId)).toEqual(['a2']);
  });

  it('updates subagent lifecycle and turn status', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'subagent.started', {
        subagentId: 's1',
        description: '分析离线',
        subagentType: 'general-purpose',
      }),
    );
    model.ingestEvent(
      event(2, 'subagent.completed', {
        subagentId: 's1',
        status: 'completed',
        summary: '2 台 unknown',
      }),
    );
    const subagent = model.blocks.find((block) => block.kind === 'subagent');
    expect(subagent).toMatchObject({ id: 's1', status: 'completed', summary: '2 台 unknown' });

    model.ingestEvent(event(3, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(
      event(4, 'turn/end', { turnId: 't1', outcome: 'completed', usage: null }),
    );
    const turn = model.blocks.find((block) => block.kind === 'turn');
    expect(turn).toMatchObject({ status: 'completed' });
  });
  it('keeps the trigger origin and turn execution metrics', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'trigger.received', { trigger: 'schedule', sourceId: 'wake-123456789' }),
    );
    model.ingestEvent(event(2, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(
      event(3, 'tool/call', {
        toolCallId: 'c1',
        tool: 'read_device',
        args: { thingId: 'dev-1' },
      }),
    );
    model.ingestEvent(
      event(4, 'tool/result', {
        toolCallId: 'c1',
        tool: 'read_device',
        result: '{"online":true}',
      }),
    );
    model.ingestEvent(
      event(5, 'turn/end', {
        turnId: 't1',
        outcome: 'completed',
        toolCalls: 16,
        usage: { inputTokens: 76064, cachedInputTokens: 61442, outputTokens: 2973, costUsd: 0 },
      }),
    );

    expect(model.blocks.find((block) => block.kind === 'origin')).toMatchObject({
      source: 'schedule',
      sourceId: 'wake-123456789',
    });
    expect(model.blocks.find((block) => block.kind === 'turn')).toMatchObject({
      status: 'completed',
      toolCalls: 16,
      usage: { inputTokens: 76064, outputTokens: 2973 },
      completedAt: expect.any(String),
    });
  });

  it('marks failed tool results with the structured error', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'tool/call', {
        toolCallId: 'c1',
        tool: 'get_skill',
        args: { skill_name: 'patrol' },
      }),
    );
    model.ingestEvent(
      event(2, 'tool/result', {
        toolCallId: 'c1',
        tool: 'get_skill',
        result: '{"error":"Skill patrol not found","success":false}',
      }),
    );
    expect(model.blocks.find((block) => block.kind === 'tool')).toMatchObject({
      status: 'failed',
      error: 'Skill patrol not found',
    });
  });

  it('coalesces step.text deltas and finalizes them with message.created', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(event(2, 'step.text', { turnId: 't1', text: '先看' }));
    model.ingestEvent(event(3, 'step.text', { turnId: 't1', text: '设备。' }));
    let assistant = model.blocks.find((block) => block.kind === 'assistant');
    expect(assistant).toMatchObject({ text: '先看设备。', streaming: true, turnId: 't1' });

    model.ingestEvent(
      event(4, 'message.created', {
        turnId: 't1',
        role: 'assistant',
        text: '先看设备。结论如下。',
      }),
    );
    const assistants = model.blocks.filter((block) => block.kind === 'assistant');
    expect(assistants).toHaveLength(1);
    assistant = assistants[0];
    expect(assistant).toMatchObject({
      text: '先看设备。',
      streaming: false,
      turnId: 't1',
    });
  });
  it('projects a compact assistant settlement with its reasoning', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(
      event(2, 'assistant/message', {
        turnId: 't1',
        stepId: 't1:s1',
        messageId: 'm1',
        text: '冷却塔 A 离线。',
        reasoning: '先确认在线状态。',
        stream: [
          { kind: 'reasoning', atMs: 10, text: '先确认在线状态。' },
          { kind: 'text', atMs: 40, text: '冷却塔 A 离线。' },
        ],
      }),
    );
    const reasoning = model.blocks.find((block) => block.kind === 'reasoning');
    const assistant = model.blocks.find((block) => block.kind === 'assistant');
    expect(reasoning).toMatchObject({ text: '先确认在线状态。', turnId: 't1', streaming: false });
    expect(assistant).toMatchObject({
      text: '冷却塔 A 离线。',
      turnId: 't1',
      stepId: 't1:s1',
    });
  });

  it('merges live deltas and replaces them with the durable settlement', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.applyLive({
      v: 2,
      frame: 'stream/delta',
      streamId: 't1:s1',
      turnId: 't1',
      stepId: 't1:s1',
      index: 0,
      kind: 'text',
      text: '冷却塔',
    });
    model.applyLive({
      v: 2,
      frame: 'stream/delta',
      streamId: 't1:s1',
      turnId: 't1',
      stepId: 't1:s1',
      index: 0,
      kind: 'text',
      text: ' A 离线',
    });
    expect(model.blocks.filter((block) => block.kind === 'assistant')).toHaveLength(1);
    model.ingestEvent(
      event(2, 'assistant/message', {
        turnId: 't1',
        stepId: 't1:s1',
        messageId: 'm1',
        text: '冷却塔 A 离线。',
        reasoning: '',
        stream: [],
      }),
    );
    const assistants = model.blocks.filter((block) => block.kind === 'assistant');
    expect(assistants).toHaveLength(1);
    expect(assistants[0]).toMatchObject({ text: '冷却塔 A 离线。', id: 'm1', stepId: 't1:s1' });
  });

  it('keeps a question block after the turn closes without an answer', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'question.asked', {
        questionId: 'q1',
        questions: [{ header: '重连授权', question: '是否继续？', options: [], multiSelect: false }],
      }),
    );
    const question = model.blocks.find((block) => block.kind === 'question');
    expect(question).toMatchObject({ questionId: 'q1', outcome: 'pending' });
    model.ingestEvent(event(2, 'turn/end', { turnId: 't1', outcome: 'cancelled' }));
    expect(question).toMatchObject({ outcome: 'timed_out' });
    expect(model.pendingQuestion).toBeNull();
  });

  it('projects an assistant attempt as retry evidence', () => {
    const model = new TimelineModel();
    model.ingestEvent(
      event(1, 'assistant/attempt', {
        turnId: 't1',
        stepId: 't1:s1',
        attemptId: 'a1',
        attempt: 1,
        maxAttempts: 3,
        stream: [],
        error: 'HTTP 429',
        retryDelayMs: 1000,
      }),
    );
    const retry = model.blocks.find((block) => block.kind === 'retry');
    expect(retry).toMatchObject({ attempt: 1, maxRetries: 3, delayMs: 1000, error: 'HTTP 429' });
  });

  it('rolls back only the failed model attempt and records retry details', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(event(2, 'tool/call', { toolCallId: 'c1', tool: 'read_device', args: {} }));
    model.ingestEvent(
      event(3, 'tool/result', { toolCallId: 'c1', tool: 'read_device', result: 'online' }),
    );
    model.ingestEvent(event(4, 'step.text', { turnId: 't1', text: '失效的部分正文' }));
    model.ingestEvent(event(5, 'step.thinking', { turnId: 't1', thinking: '失效的思考' }));
    model.ingestEvent(
      event(6, 'turn.retry', {
        turnId: 't1',
        attempt: 1,
        maxRetries: 3,
        delayMs: 1000,
        error: 'HTTP 503: unavailable',
      }),
    );

    expect(model.blocks.find((block) => block.kind === 'tool')).toMatchObject({
      id: 'c1',
      result: 'online',
    });
    expect(
      model.blocks.some((block) => block.kind === 'assistant' || block.kind === 'reasoning'),
    ).toBe(false);
    expect(model.blocks.find((block) => block.kind === 'retry')).toMatchObject({
      turnId: 't1',
      attempt: 1,
      maxRetries: 3,
      delayMs: 1000,
      error: 'HTTP 503: unavailable',
    });
  });

  it('keeps cancellation reason and source after a history reload', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(
      event(2, 'turn/end', {
        turnId: 't1',
        outcome: 'cancelled',
        error: '用户主动停止本轮执行',
        source: 'user',
      }),
    );
    expect(model.blocks.find((block) => block.kind === 'turn')).toMatchObject({
      status: 'cancelled',
      error: '用户主动停止本轮执行',
      source: 'user',
    });
  });
  it('finalizes partial assistant content when the turn fails', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(event(2, 'step.text', { turnId: 't1', text: '最后一次请求的部分正文' }));
    model.ingestEvent(event(3, 'step.thinking', { turnId: 't1', thinking: '最后一次思考' }));
    model.ingestEvent(
      event(4, 'turn/end', {
        turnId: 't1',
        outcome: 'failed',
        error: 'HTTP 503: unavailable',
        source: 'system',
      }),
    );
    expect(model.blocks.find((block) => block.kind === 'assistant')).toMatchObject({
      streaming: false,
    });
    expect(model.blocks.find((block) => block.kind === 'reasoning')).toMatchObject({
      streaming: false,
    });
  });
  it('coalesces an older turn page without retaining its failed model attempt', () => {
    const model = new TimelineModel();
    model.ingest({ events: [event(10, 'turn/start', { turnId: 't2' })], highWatermark: 10 });
    model.ingest({
      events: [
        event(1, 'turn/start', { turnId: 't1' }),
        event(2, 'step.text', { turnId: 't1', text: '失败正文' }),
        event(3, 'turn.retry', { turnId: 't1', attempt: 1, maxRetries: 3 }),
        event(4, 'step.text', { turnId: 't1', text: '最终' }),
        event(5, 'step.text', { turnId: 't1', text: '回复' }),
        event(6, 'step.thinking', { turnId: 't1', thinking: '深入' }),
        event(7, 'step.thinking', { turnId: 't1', thinking: '思考' }),
      ],
      highWatermark: 10,
    });

    const assistants = model.blocks.filter((block) => block.kind === 'assistant' && block.turnId === 't1');
    expect(assistants).toHaveLength(1);
    expect(assistants[0]).toMatchObject({ text: '最终回复' });
    const reasoning = model.blocks.filter((block) => block.kind === 'reasoning' && block.turnId === 't1');
    expect(reasoning).toHaveLength(1);
    expect(reasoning[0]).toMatchObject({ text: '深入思考' });
  });

  it('does not replace current todos with an older history page', () => {
    const model = new TimelineModel();
    const current = [{ content: '当前任务', status: 'in_progress' as const, activeForm: '执行当前任务' }];
    const older = [{ content: '旧任务', status: 'completed' as const, activeForm: '执行旧任务' }];
    model.ingest({ events: [event(10, 'todo.updated', { todos: current })], highWatermark: 10 });
    model.ingest({ events: [event(1, 'todo.updated', { todos: older })], highWatermark: 10 });
    expect(model.todos).toEqual(current);
  });

  it('clears a pending question when its turn has already ended', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'question.asked', { questionId: 'q1', questions: [] }));
    model.ingestEvent(event(2, 'turn/end', { turnId: 't1', outcome: 'cancelled', source: 'schedule' }));
    expect(model.pendingQuestion).toBeNull();
  });

  it('does not clear a current question when an older turn page completes', () => {
    const model = new TimelineModel();
    model.ingest({ events: [event(10, 'question.asked', { questionId: 'q-current', questions: [] })], highWatermark: 10 });
    model.ingest({ events: [event(1, 'turn/end', { turnId: 't-old', outcome: 'cancelled' })], highWatermark: 10 });
    expect(model.pendingQuestion?.questionId).toBe('q-current');
  });

  it('does not resurrect a question from an older completed turn', () => {
    const model = new TimelineModel();
    model.ingest({ events: [event(10, 'turn/end', { turnId: 't-old', outcome: 'cancelled' })], highWatermark: 10 });
    model.ingest({ events: [event(1, 'question.asked', { questionId: 'q-old', questions: [] })], highWatermark: 10 });
    expect(model.pendingQuestion).toBeNull();
  });

  it('clears a pending approval when its turn has already ended', () => {
    const model = new TimelineModel();
    model.ingestEvent(event(1, 'turn/start', { turnId: 't1' }));
    model.ingestEvent(
      event(2, 'approval.requested', {
        turnId: 't1',
        approvalId: 'a1',
        action: 'call_device',
        summary: '重连设备',
      }),
    );
    model.ingestEvent(
      event(3, 'turn/end', {
        turnId: 't1',
        outcome: 'cancelled',
        source: 'schedule',
      }),
    );
    expect(model.pendingApprovals).toEqual([]);
  });
});
