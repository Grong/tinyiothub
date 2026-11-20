import { describe, expect, it } from 'vitest';
import { TimelineModel } from '../chat/timeline-model.js';
import type { SessionEvent } from '../../api/assistant.js';
import { buildStory, deviceHealthLabel, deviceTypeLabel, failureCopy } from './story-model.js';
import { buildMonthBuckets, buildWeekBuckets, computeAchievements } from './story-achievements.js';
import type { BrainEvent } from '../../api/brain-events.js';

function event(seq: number, eventType: string, payloadJson: Record<string, unknown>): SessionEvent {
  return {
    seq,
    eventId: `e${seq}`,
    sessionId: 'agent:ws:a/s1',
    executionMode: 'production',
    runId: 'run1',
    taskId: null,
    eventType,
    occurredAt: '2026-11-19T00:00:00Z',
    causeId: null,
    schemaV: 1,
    payloadDigest: 'd',
    payloadJson,
  };
}

function timelineOf(...events: SessionEvent[]): TimelineModel {
  const model = new TimelineModel();
  for (const item of events) model.ingestEvent(item);
  return model;
}

const deviceResult = {
  v: 1,
  thingId: 'dev-1',
  name: '冷却塔 A',
  thingType: 'device',
  health: 'bad',
  zone: '3 号楼屋面',
  online: false,
  state: { 出水温度: '32.5', 风机转速: '0' },
  text: '冷却塔 A 位于 3 号楼屋面，健康度 bad，离线',
};

const evidence = [
  { evidenceId: 'ev-1', mime: 'image/jpeg', sourceSeq: 8, mediaTime: '2026-11-19T00:02:00Z' },
];

describe('buildStory', () => {
  it('translates a device read into a business step, device card and vision card', () => {
    const model = timelineOf(
      event(1, 'trigger/received', { trigger: 'schedule', sourceId: 'wake-1' }),
      event(2, 'turn/start', { turnId: 't1' }),
      event(3, 'tool/call', {
        toolCallId: 'c1',
        tool: 'read_device',
        args: { thingId: 'dev-1' },
      }),
      event(4, 'tool/result', {
        toolCallId: 'c1',
        tool: 'read_device',
        result: JSON.stringify(deviceResult),
        evidence,
      }),
      event(5, 'assistant/message', {
        messageId: 'm1',
        text: '冷却塔 A 离线，已通知值班人员。',
      }),
      event(6, 'turn/end', { turnId: 't1', outcome: 'completed', toolCalls: 1 }),
    );

    const story = buildStory(model.blocks, []);

    expect(story.sections).toHaveLength(1);
    expect(story.origin).toBe('schedule');
    expect(story.sections[0].conclusion).toContain('冷却塔 A 离线');
    expect(story.sections[0].steps).toHaveLength(1);

    expect(story.devices).toHaveLength(1);
    expect(story.devices[0]).toMatchObject({
      id: 'dev-1',
      name: '冷却塔 A',
      online: false,
      health: 'bad',
      zone: '3 号楼屋面',
    });
    expect(story.devices[0].metrics).toHaveLength(2);

    expect(story.sections[0].steps[0]).toMatchObject({
      title: '查看「冷却塔 A」的实时状态',
      status: 'done',
      tone: 'read',
      deviceId: 'dev-1',
      finding: '设备离线',
    });
    expect(story.sections[0].steps[0].detail).toContain('离线');
    expect(story.sections[0].steps[0].detail).toContain('2 项运行参数');

    expect(story.vision).toHaveLength(1);
    expect(story.vision[0]).toMatchObject({
      evidenceId: 'ev-1',
      deviceId: 'dev-1',
      deviceName: '冷却塔 A',
      caption: '现场画面',
    });
    expect(story.findings).toEqual([
      {
        deviceId: 'dev-1',
        deviceName: '冷却塔 A',
        label: '设备离线',
        at: '2026-11-19T00:00:00Z',
      },
    ]);
    expect(story.stats).toEqual({
      devices: 1,
      inspected: 1,
      vision: 1,
      findings: 1,
      actions: 0,
      steps: 1,
    });
  });

  it('summarizes a device inventory without pretending every item was inspected', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'tool/call', {
        toolCallId: 'c1',
        tool: 'list_things',
        args: { thingType: 'device' },
      }),
      event(3, 'tool/result', {
        toolCallId: 'c1',
        tool: 'list_things',
        result: JSON.stringify({
          items: [
            { id: 'd1', name: '送风机 1' },
            { id: 'd2', name: '送风机 2' },
          ],
          total: 2,
        }),
      }),
    );

    const story = buildStory(model.blocks, []);

    expect(story.sections[0].steps[0].title).toBe('梳理需要关注的设备');
    expect(story.sections[0].steps[0].detail).toBe('共 2 台');
    expect(story.devices.map((device) => device.name)).toEqual(['送风机 1', '送风机 2']);
    expect(story.stats.vision).toBe(0);
    expect(story.stats.inspected).toBe(0);
    expect(story.stats.actions).toBe(0);
  });

  it('marks failed operations and keeps the business-language error', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'tool/call', {
        toolCallId: 'c1',
        tool: 'invoke_action',
        args: { thingId: 'dev-2', actionName: 'start' },
      }),
      event(3, 'tool/result', {
        toolCallId: 'c1',
        tool: 'invoke_action',
        result: JSON.stringify({ success: false, error: '设备正被占用' }),
      }),
    );

    const story = buildStory(model.blocks, []);
    const step = story.sections[0].steps[0];

    expect(step.status).toBe('failed');
    expect(step.finding).toBe('操作未成功');
    expect(step.detail).toContain('设备正被占用');
    expect(step.tone).toBe('write');
    expect(story.stats.findings).toBe(1);
  });

  it('surfaces per-frame detection findings as business issues', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'tool/call', {
        toolCallId: 'c1',
        tool: 'read_device',
        args: { thingId: 'cam-1', sources: ['cam-1'] },
      }),
      event(3, 'tool/result', {
        toolCallId: 'c1',
        tool: 'read_device',
        result: JSON.stringify({
          thingId: 'cam-1',
          name: '屋面摄像头 02',
          thingType: 'device',
          health: 'ok',
          online: true,
          sources: [
            {
              source: 'cam-1',
              dataKind: 'detections',
              text: '画面右侧发现明火与烟雾',
              data: [{ label: 'fire' }],
            },
          ],
        }),
      }),
    );

    const story = buildStory(model.blocks, []);
    expect(story.sections[0].steps[0].finding).toBe('画面：画面右侧发现明火与烟雾');
    expect(story.stats.findings).toBe(1);
  });

  it('deduplicates repeated findings for the same device', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'tool/call', {
        toolCallId: 'c1',
        tool: 'read_device',
        args: { thingId: 'dev-1' },
      }),
      event(3, 'tool/result', {
        toolCallId: 'c1',
        tool: 'read_device',
        result: JSON.stringify(deviceResult),
      }),
      event(4, 'tool/call', {
        toolCallId: 'c2',
        tool: 'read_device',
        args: { thingId: 'dev-1' },
      }),
      event(5, 'tool/result', {
        toolCallId: 'c2',
        tool: 'read_device',
        result: JSON.stringify(deviceResult),
      }),
    );

    const story = buildStory(model.blocks, []);
    expect(story.sections[0].findingCount).toBe(1);
    expect(story.stats.findings).toBe(1);
  });

  it('keeps the right rail scoped to the latest round', () => {
    const firstRound = [
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'tool/call', {
        toolCallId: 'c1',
        tool: 'read_device',
        args: { thingId: 'dev-1' },
      }),
      event(3, 'tool/result', {
        toolCallId: 'c1',
        tool: 'read_device',
        result: JSON.stringify(deviceResult),
        evidence,
      }),
      event(4, 'turn/end', { turnId: 't1', outcome: 'completed' }),
    ];
    const secondRound = [
      event(5, 'turn/start', { turnId: 't2' }),
      event(6, 'tool/call', {
        toolCallId: 'c2',
        tool: 'read_device',
        args: { thingId: 'dev-2' },
      }),
      event(7, 'tool/result', {
        toolCallId: 'c2',
        tool: 'read_device',
        result: JSON.stringify({
          thingId: 'dev-2',
          name: '循环泵 1#',
          thingType: 'device',
          health: 'ok',
          zone: 'B1 泵房',
          online: true,
          state: { 出口压力: '0.62' },
        }),
        evidence: [
          {
            evidenceId: 'ev-2',
            mime: 'image/jpeg',
            sourceSeq: 9,
            mediaTime: '2026-11-19T00:03:00Z',
          },
        ],
      }),
      event(8, 'turn/end', { turnId: 't2', outcome: 'completed' }),
    ];

    const story = buildStory(timelineOf(...firstRound, ...secondRound).blocks, []);
    expect(story.devices).toHaveLength(2);
    expect(story.vision).toHaveLength(2);
    expect(story.latestSectionId).toBe('t2');
    expect(story.latestDevices.map((device) => device.name)).toEqual(['循环泵 1#']);
    expect(story.latestVision.map((item) => item.evidenceId)).toEqual(['ev-2']);
    expect(story.latestStats).toEqual({
      devices: 1,
      inspected: 1,
      vision: 1,
      findings: 0,
      actions: 0,
    });
  });

  it('does not invent zero counts when a tool result is truncated', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'tool/call', { toolCallId: 'c1', tool: 'list_things', args: {} }),
      event(3, 'tool/result', {
        toolCallId: 'c1',
        tool: 'list_things',
        result: '{"items":[{"id":"d1","name":"送风机',
      }),
    );

    const story = buildStory(model.blocks, []);
    expect(story.sections[0].steps[0].detail).toBe('已完成');
  });

  it('keeps the latest plan and the reasoning used to reach it', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't1' }),
      event(2, 'assistant/message', {
        turnId: 't1',
        stepId: 't1:s1',
        messageId: 'm1',
        text: '',
        reasoning: '先看离线设备，再看告警。',
        stream: [{ kind: 'reasoning', atMs: 1, text: '先看离线设备，再看告警。' }],
      }),
      event(3, 'todo/write', {
        todos: [
          { content: '检查离线设备', status: 'completed', activeForm: '检查离线设备' },
          { content: '确认告警', status: 'in_progress', activeForm: '确认告警' },
        ],
      }),
    );

    const story = buildStory(model.blocks, [
      { content: '检查离线设备', status: 'completed', activeForm: '检查离线设备' },
      { content: '确认告警', status: 'in_progress', activeForm: '确认告警' },
    ]);

    expect(story.plans).toHaveLength(2);
    expect(story.sections[0].reasoning).toContain('先看离线设备');
    expect(story.sections[0].steps).toContainEqual(
      expect.objectContaining({ title: 'AI 在思考', tone: 'think', status: 'done' }),
    );
  });

  it('keeps an unanswered question visible after the turn ends', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't-q' }),
      event(2, 'question/asked', {
        questionId: 'q-closed',
        questions: [
          {
            header: '重连授权',
            question: '是否继续重连？',
            multiSelect: false,
            options: [],
          },
        ],
      }),
      event(3, 'turn/end', { turnId: 't-q', outcome: 'cancelled' }),
    );
    const story = buildStory(model.blocks, []);
    const latest = story.sections[story.sections.length - 1];
    const waiting = latest?.steps.find((step) => step.title.includes('重连授权'));
    expect(waiting?.title).toContain('确认未完成');
    expect(waiting?.status).toBe('failed');
  });

  it('surfaces a pending question as a waiting step', () => {
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't-wait' }),
      event(2, 'assistant/message', { turnId: 't-wait', messageId: 'm-wait', text: '诊断完成，需要确认操作范围。' }),
      event(3, 'question/asked', {
        questionId: 'q-1',
        questions: [
          {
            header: '重连授权',
            question: '是否执行离线传感器重连？',
            multiSelect: false,
            options: [
              { label: '仅网关 restart', description: '只重启网关并复查' },
              { label: '全部执行', description: '21 次写操作' },
            ],
          },
        ],
      }),
    );
    const story = buildStory(model.blocks, [], {
      questionId: 'q-1',
      at: '2026-11-20T09:09:37.000Z',
      questions: [
        {
          header: '重连授权',
          question: '是否执行离线传感器重连？',
          multiSelect: false,
          options: [
            { label: '仅网关 restart', description: '只重启网关并复查' },
            { label: '全部执行', description: '21 次写操作' },
          ],
        },
      ],
    });
    const latest = story.sections[story.sections.length - 1];
    const waiting = latest?.steps.find((step) => step.title.includes('重连授权'));
    expect(waiting?.status).toBe('running');
    expect(waiting?.detail).toContain('离线传感器重连');
  });

  it('turns a failed turn into business copy and one retry summary', () => {
    const quota =
      'CompletionError: ProviderError: LLM streaming request failed: HTTP 429: rate_limit_error: 已达到 Token Plan 用量上限';
    const model = timelineOf(
      event(1, 'turn/start', { turnId: 't-fail' }),
      event(2, 'assistant/attempt', {
        turnId: 't-fail',
        stepId: 't-fail:s1',
        attemptId: 'a1',
        attempt: 1,
        maxAttempts: 4,
        retryDelayMs: 1000,
        error: quota,
      }),
      event(3, 'assistant/attempt', {
        turnId: 't-fail',
        stepId: 't-fail:s1',
        attemptId: 'a2',
        attempt: 2,
        maxAttempts: 4,
        retryDelayMs: 2000,
        error: quota,
      }),
      event(4, 'turn/end', {
        turnId: 't-fail',
        outcome: 'failed',
        error: quota,
        modelCalls: 0,
        toolCalls: 0,
      }),
      event(5, 'session/end', { outcome: 'failed' }),
      event(6, 'turn/start', { turnId: 't-next' }),
      event(7, 'turn/end', { turnId: 't-next', outcome: 'completed' }),
    );

    const story = buildStory(model.blocks, []);
    const failed = story.sections[0];
    expect(failed.status).toBe('failed');
    expect(failed.retries).toBe(2);
    expect(failureCopy(quota).category).toBe('quota');
    expect(failed.steps.map((step) => step.title)).toContain('模型调用重试');
    const failure = failed.steps.find((step) => step.title === '本轮未完成');
    expect(failure?.detail).toContain('模型额度不足');
    expect(failure?.detail).toContain('已重试 2 次');
    expect(failed.steps.some((step) => step.title.includes('会话结束：failed'))).toBe(false);
  });

  it('exports shared labels for device cards', () => {
    expect(deviceTypeLabel('device')).toBe('设备');
    expect(deviceTypeLabel('space')).toBe('空间');
    expect(deviceHealthLabel('attention')).toBe('需要关注');
    expect(deviceHealthLabel(undefined)).toBe('');
  });
});

function brainEvent(overrides: Partial<BrainEvent>): BrainEvent {
  return {
    id: 'alarm:a1',
    source: 'alarm',
    alarmId: null,
    thingId: null,
    title: '告警',
    verdict: null,
    reason: '',
    suggestedAction: null,
    actionCategory: null,
    risk: null,
    runId: null,
    tickId: null,
    ticketId: null,
    status: 'resolved',
    triageMode: 'auto',
    createdAt: '2026-11-19T00:00:00Z',
    judgedAt: null,
    stateEnteredAt: null,
    resolvedAt: null,
    ...overrides,
  };
}

describe('achievements', () => {
  const now = new Date(2026, 10, 19, 12, 0, 0);

  it('buckets the last seven local days and counts outcomes', () => {
    const events = [
      brainEvent({ id: 'a', status: 'resolved', resolvedAt: '2026-11-19T02:00:00Z' }),
      brainEvent({ id: 'b', status: 'self_closed', resolvedAt: '2026-11-18T02:00:00Z' }),
      brainEvent({ id: 'c', status: 'patrol_ok', judgedAt: '2026-11-19T01:00:00Z' }),
      brainEvent({ id: 'd', status: 'escalated', stateEnteredAt: '2026-11-17T01:00:00Z' }),
      brainEvent({ id: 'e', status: 'dismissed', createdAt: '2026-10-01T00:00:00Z' }),
    ];

    const days = buildWeekBuckets(events, now);
    expect(days).toHaveLength(7);
    expect(days[6].key).toBe('2026-11-19');
    expect(days[6].closed).toBe(1);
    expect(days[6].patrol).toBe(1);
    expect(days[6].total).toBe(2);
  });

  it('buckets the current month by week segments', () => {
    const events = [
      brainEvent({ id: 'a', status: 'resolved', resolvedAt: '2026-11-03T02:00:00Z' }),
      brainEvent({ id: 'b', status: 'patrol_ok', judgedAt: '2026-11-12T02:00:00Z' }),
      brainEvent({ id: 'c', status: 'resolved', resolvedAt: '2026-11-19T02:00:00Z' }),
      brainEvent({ id: 'd', status: 'resolved', resolvedAt: '2026-10-31T02:00:00Z' }),
    ];
    const buckets = buildMonthBuckets(events, now);
    expect(buckets).toHaveLength(5);
    expect(buckets[0].label).toBe('1-7');
    expect(buckets[0].closed).toBe(1);
    expect(buckets[1].closed).toBe(0);
    expect(buckets[1].patrol).toBe(1);
    expect(buckets[2].closed).toBe(1);
    expect(buckets.reduce((sum, bucket) => sum + bucket.total, 0)).toBe(3);
  });

  it('separates this shift, today and the week without inventing savings', () => {
    const story = buildStory(
      timelineOf(
        event(1, 'turn/start', { turnId: 't1' }),
        event(2, 'tool/call', {
          toolCallId: 'c1',
          tool: 'read_device',
          args: { thingId: 'dev-1' },
        }),
        event(3, 'tool/result', {
          toolCallId: 'c1',
          tool: 'read_device',
          result: JSON.stringify(deviceResult),
          evidence,
        }),
      ).blocks,
      [],
    );

    const stats = computeAchievements({
      story,
      events: [
        brainEvent({ id: 'a', status: 'resolved', resolvedAt: '2026-11-19T02:00:00Z' }),
        brainEvent({ id: 'b', status: 'self_closed', resolvedAt: '2026-11-18T02:00:00Z' }),
        brainEvent({ id: 'c', status: 'patrol_ok', judgedAt: '2026-11-19T01:00:00Z' }),
        brainEvent({
          id: 'd',
          status: 'awaiting_approval',
          stateEnteredAt: '2026-11-19T03:00:00Z',
        }),
      ],
      summary: {
        digestedToday: 6,
        needsYou: 1,
        latencyP50Secs: null,
        feedbackRight: 0,
        feedbackWrong: 0,
      },
      storyAt: new Date(2026, 10, 19, 9, 0, 0).getTime(),
      now,
    });

    expect(stats.latest).toEqual({ devices: 1, inspected: 1, vision: 1, findings: 1, actions: 0 });
    expect(stats.today).toEqual({ closed: 1, patrol: 1, needsYou: 1, digested: 6, total: 3 });
    expect(stats.week.closed).toBe(2);
    expect(stats.week.patrol).toBe(1);
    expect(stats.week.needsYou).toBe(1);
    expect(stats.week.total).toBe(4);
    expect(stats.month.closed).toBe(2);
    expect(stats.month.patrol).toBe(1);
    expect(stats.month.total).toBe(4);
  });

  it('does not count a past shift as today', () => {
    const story = buildStory([], []);
    const stats = computeAchievements({
      story: {
        ...story,
        latestStats: { devices: 8, inspected: 5, vision: 3, findings: 2, actions: 1 },
      },
      events: [],
      summary: null,
      storyAt: new Date(2026, 10, 18, 9, 0, 0).getTime(),
      now,
    });
    expect(stats.latest.devices).toBe(0);
  });
});
