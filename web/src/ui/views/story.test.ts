import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LiveFrame } from '../../types/session-protocol.js';

const hoisted = vi.hoisted(() => ({
  fetchSessionEvents: vi.fn(),
  connectSessionEvents: vi.fn(
    (_opts: {
      sessionKey: string;
      after: number;
      onEvent: (event: SessionEvent) => void;
      onLive: (frame: LiveFrame) => void;
    }) =>
      () => {},
  ),
  fetchSessionTodos: vi.fn(),
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
    hoisted.fetchSessionTodos.mockResolvedValue([]);
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
            thingId: 'dev-1',
            name: '冷却塔 A',
            thingType: 'device',
            health: 'attention',
            zone: '3 号楼屋面',
            online: true,
            state: { 出水温度: '32.5' },
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

  it('renders the business story, device deck and brain achievements', async () => {
    const el = document.createElement('view-story') as StoryView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.textContent).toContain('冷却塔 A'));

    expect(el.textContent).toContain('AI 工作纪实');
    expect(el.textContent).toContain('3 号楼夜间巡检');
    expect(el.textContent).toContain('查看「冷却塔 A」的实时状态');
    expect(hoisted.fetchSessionEvents.mock.calls[0][3]).toEqual({ turns: 2 });
    expect(el.textContent).toContain('AI 大脑功绩');
    expect(el.textContent).toContain('本轮发现');
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
          thingId: id,
          name,
          thingType: 'device',
          health: 'ok',
          zone: '3 号楼',
          online: true,
          state: { 温度: '26' },
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

  it('does not let a stale active session mask a newer conversation', () => {
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
    expect(pickDefaultSession([staleActive, newerChat])?.sessionKey).toBe(newerChat.sessionKey);
  });
});
