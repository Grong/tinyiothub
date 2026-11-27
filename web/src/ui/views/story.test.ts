import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LiveFrame } from '../../types/session-protocol.js';

const hoisted = vi.hoisted(() => ({
  fetchSessionEvents: vi.fn(),
  fetchSessionProjection: vi.fn(),
  connectSessionEvents: vi.fn(
    (_opts: {
      sessionKey: string;
      after: number;
      onEvent: (event: SessionEvent) => void;
      onLive: (frame: LiveFrame) => void;
    }) =>
      () => {},
  ),
  listChatSessions: vi.fn(),
  fetchAssistantHud: vi.fn(),
  getWorkspaceId: vi.fn(() => 'ws-1'),
  brainList: vi.fn(),
  brainSummary: vi.fn(),
}));

vi.mock('../../api/assistant.js', () => hoisted);
vi.mock('../../api/client.js', async () => {
  const actual = await vi.importActual<typeof import('../../api/client.js')>('../../api/client.js');
  return { ...actual, getWorkspaceId: hoisted.getWorkspaceId };
});
vi.mock('../../api/brain-events.js', () => ({
  brainEventApi: { list: hoisted.brainList, summary: hoisted.brainSummary },
}));

import './story.js';
import { pickDefaultSession, type StoryView } from './story.js';
import type { SessionEvent, SessionSummary } from '../../api/assistant.js';

const session: SessionSummary = {
  sessionKey: 'agent:ws-1:default/s1',
  workspaceId: 'ws-1',
  agentId: 'default',
  label: '3 号楼夜间巡检',
  kind: 'scheduled',
  status: 'ended',
  permissionMode: 'ask',
  createdAt: Date.now() - 60_000,
  updatedAt: Date.now(),
};

function event(seq: number, eventType: string, payloadJson: Record<string, unknown>) {
  if (payloadJson.tool === 'read_device' && !payloadJson.viewKey) {
    payloadJson = { ...payloadJson, viewKey: 'device.read' };
  }
  return {
    seq,
    eventId: `e${seq}`,
    sessionId: 's',
    executionMode: 'production',
    runId: 'r',
    taskId: null,
    eventType,
    occurredAt: new Date().toISOString(),
    causeId: null,
    schemaV: 1,
    payloadDigest: 'd',
    payloadJson,
  };
}

describe('view-story', () => {
  beforeEach(() => {
    localStorage.setItem('workspace-id', 'ws-1');
    hoisted.listChatSessions.mockResolvedValue([session]);
    hoisted.fetchAssistantHud.mockResolvedValue({
      assistant: { status: 'idle', lastOutcome: 'completed' },
      devices: { total: 12, online: 10, offline: 2 },
      alarms: { highPriority: 0 },
      pending: { questions: 0, approvals: 0 },
      activity: { activeSessions: 0 },
      schedule: {
        enabled: true,
        intervalSeconds: 300,
        prompt: '',
        nextWakeAt: null,
        lastWakeAt: null,
        lastOutcome: null,
      },
      recentEvents: [],
    });
    hoisted.brainList.mockResolvedValue([]);
    hoisted.brainSummary.mockResolvedValue({
      digestedToday: 4,
      needsYou: 0,
      latencyP50Secs: null,
      feedbackRight: 0,
      feedbackWrong: 0,
    });
    hoisted.connectSessionEvents.mockReturnValue(() => {});
    hoisted.fetchSessionProjection.mockResolvedValue({
      turn: { turnId: 't1', phase: 'executing', agenda: [], lastSeq: 6 },
      highWatermark: 6,
    });
    hoisted.fetchSessionEvents.mockResolvedValue({
      events: [
        event(1, 'trigger/received', { trigger: 'schedule' }),
        event(2, 'turn/start', { turnId: 't1' }),
        event(3, 'tool/call', {
          toolCallId: 'c1',
          tool: 'read_device',
          args: { thingId: 'dev-1' },
        }),
        event(4, 'tool/result', {
          toolCallId: 'c1',
          tool: 'read_device',
          result: JSON.stringify({
            thing: {
              id: 'dev-1',
              name: 'cooling_tower_a',
              displayName: '冷却塔 A',
              thingType: 'device',
              health: 'attention',
              online: true,
            },
            properties: [{ name: 'outlet_temperature', displayName: '出水温度', currentValue: '32.5', unit: '°C' }],
            commands: [],
            zone: '3 号楼屋面',
          }),
        }),
        event(5, 'assistant/message', {
          messageId: 'm1',
          text: '冷却塔 A 运行正常，出水温度 32.5℃。',
        }),
        event(6, 'turn/end', { turnId: 't1', outcome: 'completed', toolCalls: 1 }),
      ],
      highWatermark: 6,
    });
  });

  afterEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    Object.values(hoisted).forEach((mock) => mock.mockReset());
    hoisted.getWorkspaceId.mockReturnValue('ws-1');
  });

  it('restores the last story session instead of jumping to the newest one', async () => {
    const ticket = {
      ...session,
      sessionKey: 'agent:ws-1:default/ticket-5',
      label: '工单 #5 车间烟雾火警：烟雾浓度 当前值 7.98（阈值 > 5）',
      kind: 'event' as const,
      updatedAt: Date.now() + 10_000,
    };
    hoisted.listChatSessions.mockResolvedValue([session, ticket]);
    localStorage.setItem('tinyiothub_story_last_session', session.sessionKey);
    localStorage.setItem('tinyiothub_story_session_manual', '1');
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(hoisted.fetchSessionEvents).toHaveBeenCalled());
    expect(hoisted.fetchSessionEvents.mock.calls[0][0]).toBe(session.sessionKey);
    expect(el.querySelector('.story-hero__title')?.textContent).not.toContain('工单 #5');
  });

  it('ignores a legacy ticket selection and falls back to the scheduled shift', async () => {
    const ticket = {
      ...session,
      sessionKey: 'agent:ws-1:default/ticket-5',
      label: '工单 #5 车间烟雾火警',
      kind: 'event' as const,
      updatedAt: Date.now() + 10_000,
    };
    hoisted.listChatSessions.mockResolvedValue([ticket, session]);
    localStorage.setItem('tinyiothub_story_last_session', ticket.sessionKey);
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(hoisted.fetchSessionEvents).toHaveBeenCalled());
    expect(hoisted.fetchSessionEvents.mock.calls[0][0]).toBe(session.sessionKey);
  });

  it('renders a pending approval inline with direct actions', async () => {
    hoisted.fetchSessionEvents.mockResolvedValueOnce({
      events: [
        event(1, 'trigger/received', { trigger: 'schedule' }),
        event(2, 'turn/start', { turnId: 't-approval' }),
        event(3, 'approval/requested', {
          turnId: 't-approval',
          approvalId: 'a1',
          action: 'set_device',
          summary: '将冷却塔 A 的目标温度调整为 26°C',
        }),
      ],
      highWatermark: 3,
    });
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.querySelector('approval-card')).toBeTruthy());
    expect(el.querySelector('.story-waiting--approval')).toBeTruthy();
  });

  it('catches up events from the projection high watermark', async () => {
    hoisted.fetchSessionProjection.mockResolvedValueOnce({
      turn: { turnId: 't1', phase: 'completed', agenda: [], lastSeq: 6 },
      highWatermark: 7,
    });
    hoisted.fetchSessionEvents
      .mockResolvedValueOnce({
        events: [event(1, 'turn/start', { turnId: 't1' })],
        highWatermark: 1,
      })
      .mockResolvedValueOnce({
        events: [event(7, 'turn/end', { turnId: 't1', outcome: 'completed', text: 'done' })],
        highWatermark: 7,
      });
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(hoisted.fetchSessionEvents).toHaveBeenCalledTimes(2));
    expect(hoisted.fetchSessionEvents.mock.calls[1][1]).toBe(1);
  });

  it('renders a pending question with the inline question card', async () => {
    hoisted.fetchSessionProjection.mockResolvedValueOnce({
      turn: { turnId: 't-question', phase: 'executing', agenda: [], lastSeq: 2 },
      highWatermark: 2,
    });
    hoisted.fetchSessionEvents.mockResolvedValueOnce({
      events: [
        event(1, 'turn/start', { turnId: 't-question' }),
        event(2, 'question/asked', {
          turnId: 't-question',
          questionId: 'q1',
          questions: [
            {
              header: '执行范围',
              question: '是否对这两台设备执行重连？',
              options: [{ label: '执行' }, { label: '跳过' }],
              multiSelect: false,
            },
          ],
        }),
      ],
      highWatermark: 2,
    });
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.querySelector('question-card')).toBeTruthy());
    expect(el.querySelector('.story-waiting--question')).toBeTruthy();
  });

  it('renders projection todos in the bottom dock and plan diffs inside details', async () => {
    hoisted.fetchSessionProjection.mockResolvedValueOnce({
      turn: {
        turnId: 't-plan',
        phase: 'executing',
        agenda: [],
        todos: [
          { content: '扫描告警', status: 'completed' },
          { content: '生成摘要', status: 'in_progress' },
        ],
        lastSeq: 8,
      },
      highWatermark: 8,
    });
    const initial = [
      { content: '扫描告警', status: 'in_progress' },
      { content: '生成摘要', status: 'pending' },
    ];
    const next = [
      { content: '扫描告警', status: 'completed' },
      { content: '生成摘要', status: 'in_progress' },
    ];
    hoisted.fetchSessionEvents.mockResolvedValueOnce({
      events: [
        event(1, 'turn/start', { turnId: 't-plan' }),
        event(2, 'todo/write', { todos: initial }),
        event(3, 'tool/call', { toolCallId: 'p1', tool: 'todo_write', args: { todos: initial } }),
        event(4, 'tool/result', { toolCallId: 'p1', tool: 'todo_write', result: '{"ok":true}' }),
        event(5, 'todo/write', { todos: next }),
        event(6, 'tool/call', { toolCallId: 'p2', tool: 'todo_write', args: { todos: next } }),
        event(7, 'tool/result', { toolCallId: 'p2', tool: 'todo_write', result: '{"ok":true}' }),
        event(8, 'turn/end', { turnId: 't-plan', outcome: 'completed' }),
      ],
      highWatermark: 8,
    });
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.querySelector('.story-task-dock')).toBeTruthy());
    expect(el.querySelector('.story-task-dock')?.textContent).toContain('已完成 1 · 1 进行中');
    expect(el.textContent).not.toContain('本轮计划');
    const planStep = [...el.querySelectorAll('.story-step')]
      .filter((node) => node.textContent?.includes('更新计划'))
      .at(-1) as HTMLDetailsElement | undefined;
    expect(planStep).toBeTruthy();
    expect(el.querySelector('.story-process')).toBeTruthy();
    expect(planStep?.querySelector('.story-plan-diff')).toBeTruthy();
    expect(planStep?.querySelectorAll('.story-plan-diff__item').length).toBe(2);
  });

  it('opens the session switcher menu without a native select', async () => {
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.querySelector('.story-switcher__button')).toBeTruthy());
    (el.querySelector('.story-switcher__button') as HTMLElement).click();
    await el.updateComplete;
    expect(el.querySelector('.story-switcher__menu')).toBeTruthy();
    expect(el.querySelector('.story-switcher__item')).toBeTruthy();
  });

  it('shows a jump-to-latest control after the reader scrolls up', async () => {
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.querySelector('.story-body')).toBeTruthy());
    const body = el.querySelector('.story-body') as HTMLElement;
    body.dispatchEvent(new WheelEvent('wheel', { deltaY: -120, bubbles: true }));
    await el.updateComplete;
    const jump = el.querySelector('.story-jump') as HTMLButtonElement | null;
    expect(jump).toBeTruthy();
    jump?.click();
    await el.updateComplete;
    expect(el.querySelector('.story-jump')).toBeNull();
  });

  it('renders the business story, device deck and brain achievements', async () => {
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.textContent).toContain('冷却塔 A'));

    expect(el.textContent).toContain('AI 工作纪实');
    expect(el.textContent).toContain('3 号楼夜间巡检');
    expect(el.querySelector('.story-hero__title')?.textContent).toContain('冷却塔 A');
    expect(el.querySelector('.story-hero__title')?.textContent).not.toBe('3 号楼夜间巡检');
    expect(el.textContent).toContain('读取「冷却塔 A」的实时状态');
    expect(hoisted.fetchSessionEvents.mock.calls[0][3]).toEqual({ turns: 2 });
    expect(el.textContent).toContain('AI 大脑功绩');
    expect(el.textContent).toContain('需要关注');
    const step = el.querySelector('details.story-step') as HTMLDetailsElement | null;
    expect(step).toBeTruthy();
    expect(step?.open).toBe(false);
    expect(el.querySelector('story-device-deck')).toBeTruthy();
    expect(el.querySelector('story-vision-deck')).toBeTruthy();
    expect(el.querySelector('story-achievements')).toBeTruthy();
    el.remove();
  });

  it('loads older turns on demand while keeping the latest round in the rail', async () => {
    const makeTurn = (seq: number, turnId: string, id: string, name: string) => [
      event(seq, 'turn/start', { turnId }),
      event(seq + 1, 'tool/call', {
        toolCallId: `c-${id}`,
        tool: 'read_device',
        args: { thingId: id },
      }),
      event(seq + 2, 'tool/result', {
        toolCallId: `c-${id}`,
        tool: 'read_device',
        result: JSON.stringify({
          thing: { id, name: `${name}_code`, displayName: name, thingType: 'device', health: 'ok', online: true },
          properties: [{ name: 'temperature', displayName: '温度', currentValue: '26', unit: '°C' }],
          commands: [],
          zone: '3 号楼',
        }),
      }),
      event(seq + 3, 'turn/end', { turnId, outcome: 'completed' }),
    ];
    const latest = makeTurn(10, 't-new', 'dev-new', '新风机');
    const older = makeTurn(1, 't-old', 'dev-old', '旧水泵');
    hoisted.fetchSessionEvents.mockReset();
    hoisted.fetchSessionEvents.mockImplementation(
      async (_key: string, _after: number, _limit: number, query?: { before?: number }) => {
        if (query?.before === 10) {
          return { events: older, highWatermark: 13, hasMore: false, nextBefore: null };
        }
        return { events: latest, highWatermark: 13, hasMore: true, nextBefore: 10 };
      },
    );

    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.textContent).toContain('新风机'));

    const button = el.querySelector('.story-history__button') as HTMLButtonElement | null;
    expect(button).toBeTruthy();
    button?.click();
    await vi.waitFor(() => expect(el.textContent).toContain('旧水泵'));
    const olderCall = hoisted.fetchSessionEvents.mock.calls[1];
    expect(olderCall[3]).toEqual({ turns: 1, before: 10 });
    expect(el.querySelector('story-device-deck')?.textContent).toContain('新风机');
    el.remove();
  });

  it('streams a new turn in place without reloading history', async () => {
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.textContent).toContain('冷却塔 A'));

    const fetchesBefore = hoisted.fetchSessionEvents.mock.calls.length;
    const connection = hoisted.connectSessionEvents.mock.calls[0][0];

    connection.onEvent(event(20, 'turn/start', { turnId: 't-new' }));
    await vi.waitFor(() => expect(el.textContent).not.toContain('冷却塔 A'));

    connection.onLive({
      v: 2,
      frame: 'stream/delta',
      streamId: 't-new:s1',
      turnId: 't-new',
      stepId: 't-new:s1',
      index: 0,
      kind: 'text',
      text: '正在检查',
    });
    await vi.waitFor(() => expect(el.textContent).toContain('正在检查'));

    connection.onLive({
      v: 2,
      frame: 'stream/delta',
      streamId: 't-new:s1',
      turnId: 't-new',
      stepId: 't-new:s1',
      index: 0,
      kind: 'text',
      text: '新风机。',
    });
    await vi.waitFor(() => expect(el.textContent).toContain('正在检查新风机。'));

    connection.onEvent(
      event(23, 'tool/call', {
        toolCallId: 'c-new',
        tool: 'read_device',
        args: { thingId: 'dev-new' },
      }),
    );
    connection.onEvent(
      event(24, 'tool/result', {
        toolCallId: 'c-new',
        tool: 'read_device',
        result: JSON.stringify({
          thingId: 'dev-new',
          name: '新风机',
          thingType: 'device',
          health: 'ok',
          zone: '4 号楼',
          online: true,
          state: { 温度: '26' },
        }),
      }),
    );

    await vi.waitFor(() => expect(el.textContent).toContain('新风机'));
    expect(hoisted.fetchSessionEvents.mock.calls.length).toBe(fetchesBefore);
    el.remove();
  });

  it('renders an actionable error when sessions cannot be loaded', async () => {
    hoisted.listChatSessions.mockRejectedValue(new Error('会话服务不可用'));
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.textContent).toContain('会话服务不可用'));
    el.remove();
  });
});

describe('pickDefaultSession', () => {
  it('prefers the most recently updated shift, breaking ties toward active', () => {
    const running: SessionSummary = {
      ...session,
      sessionKey: 'agent:ws-1:default/running',
      status: 'active',
      kind: 'scheduled',
      updatedAt: 600,
    };
    const chat: SessionSummary = {
      ...session,
      sessionKey: 'agent:ws-1:default/chat',
      status: 'ended',
      kind: 'chat',
      updatedAt: 500,
    };
    expect(pickDefaultSession([chat, running])?.sessionKey).toBe(running.sessionKey);
    expect(pickDefaultSession([])).toBeNull();
  });

  it('prefers the scheduled inspection over newer chat tickets', () => {
    const staleActive: SessionSummary = {
      ...session,
      sessionKey: 'agent:ws-1:default/stale',
      status: 'active',
      kind: 'scheduled',
      updatedAt: 100,
    };
    const newerChat: SessionSummary = {
      ...session,
      sessionKey: 'agent:ws-1:default/newer',
      status: 'ended',
      kind: 'chat',
      updatedAt: 900,
    };
    expect(pickDefaultSession([staleActive, newerChat])?.sessionKey).toBe(staleActive.sessionKey);
  });
});
