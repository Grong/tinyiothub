import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  fetchSessionEvents: vi.fn(),
  connectSessionEvents: vi.fn((_opts: { sessionKey: string; after: number }) => () => {}),
  fetchSessionTodos: vi.fn(),
  listChatSessions: vi.fn().mockResolvedValue([]),
  updateSessionPermission: vi.fn(),
  fetchAssistantHud: vi.fn(),
  getWorkspaceId: vi.fn(() => 'ws-1'),
  loadChatHistory: vi.fn(),
}));
vi.mock('../../api/assistant.js', () => hoisted);
vi.mock('../../api/client.js', async () => {
  const actual = await vi.importActual<typeof import('../../api/client.js')>('../../api/client.js');
  return { ...actual, getWorkspaceId: hoisted.getWorkspaceId };
});
vi.mock('../controllers/chat.js', async () => {
  const actual = await vi.importActual<typeof import('../controllers/chat.js')>(
    '../controllers/chat.js',
  );
  return { ...actual, loadChatHistory: hoisted.loadChatHistory };
});

import './chat.js';
import type { ChatView } from './chat.js';

const turnStarted = {
  seq: 1,
  eventId: 'e1',
  sessionId: 's',
  executionMode: 'production',
  runId: 'r',
  taskId: null,
  eventType: 'turn.started',
  occurredAt: '',
  causeId: null,
  schemaV: 1,
  payloadDigest: 'd',
  payloadJson: { turnId: 't1' },
};

describe('view-chat codex layout', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    localStorage.clear();
    Object.values(hoisted).forEach((mock) => mock.mockReset());
    hoisted.listChatSessions.mockResolvedValue([]);
    hoisted.connectSessionEvents.mockReturnValue(() => {});
    hoisted.getWorkspaceId.mockReturnValue('ws-1');
    hoisted.fetchSessionTodos.mockResolvedValue([]);
  });

  beforeEach(() => {
    localStorage.setItem('workspace-id', 'ws-1');
    hoisted.listChatSessions.mockResolvedValue([]);
    hoisted.connectSessionEvents.mockReturnValue(() => {});
    hoisted.getWorkspaceId.mockReturnValue('ws-1');
    hoisted.fetchSessionTodos.mockResolvedValue([]);
    hoisted.loadChatHistory.mockResolvedValue(undefined);
    hoisted.fetchAssistantHud.mockResolvedValue({
      assistant: { status: 'idle', lastOutcome: null },
      devices: { total: 0, online: 0, offline: 0 },
      alarms: { highPriority: 0 },
      pending: { questions: 0, approvals: 0 },
      activity: { activeSessions: 0 },
      schedule: { enabled: true, intervalSeconds: 300, prompt: '', nextWakeAt: null, lastWakeAt: null, lastOutcome: null },
      recentEvents: [],
    });
  });

  it('loads a session timeline and renders blocks', async () => {
    hoisted.fetchSessionEvents.mockResolvedValue({
      events: [
        turnStarted,
        {
          seq: 2,
          eventId: 'e2',
          sessionId: 's',
          executionMode: 'production',
          runId: 'r',
          taskId: null,
          eventType: 'message.created',
          occurredAt: '',
          causeId: null,
          schemaV: 1,
          payloadDigest: 'd',
          payloadJson: { messageId: 'm1', role: 'user', text: '看看 3 号楼' },
        },
      ],
      highWatermark: 2,
    });
    hoisted.fetchSessionTodos.mockResolvedValue([]);
    const el = document.createElement('view-chat') as ChatView;
    document.body.appendChild(el);
    await el.updateComplete;
    await vi.waitFor(() => expect(el.textContent).toContain('看看 3 号楼'));
    expect(el.querySelector('session-drawer')).toBeTruthy();
    expect(el.querySelector('global-hud')).toBeTruthy();
  });

  it('connects session events and reconnects on session switch', async () => {
    hoisted.fetchSessionEvents.mockResolvedValue({ events: [turnStarted], highWatermark: 1 });
    const closeFirst = vi.fn();
    hoisted.connectSessionEvents.mockReturnValueOnce(closeFirst);
    const el = document.createElement('view-chat') as ChatView;
    document.body.appendChild(el);
    await el.updateComplete;
    await vi.waitFor(() => expect(hoisted.connectSessionEvents).toHaveBeenCalled());

    const first = hoisted.connectSessionEvents.mock.calls[0][0];
    expect(first.sessionKey).toMatch(/^agent:ws-1:default\//);
    expect(first.after).toBe(0);

    el.querySelector('session-drawer')!.dispatchEvent(
      new CustomEvent('session-select', {
        detail: 'agent:ws-1:default/next',
        bubbles: true,
        composed: true,
      }),
    );
    await el.updateComplete;

    expect(closeFirst).toHaveBeenCalledTimes(1);
    expect(hoisted.connectSessionEvents).toHaveBeenCalledTimes(2);
    const second = hoisted.connectSessionEvents.mock.calls[1][0];
    expect(second.sessionKey).toBe('agent:ws-1:default/next');
    expect(second.after).toBe(0);
  });

  it('renders a2ui surfaces pushed through the chat state', async () => {
    hoisted.fetchSessionEvents.mockResolvedValue({ events: [turnStarted], highWatermark: 1 });
    const el = document.createElement('view-chat') as ChatView;
    document.body.appendChild(el);
    await el.updateComplete;
    await vi.waitFor(() => expect((el as any).chatState?.onA2ui).toBeTruthy());

    (el as any).chatState.onA2ui(
      [
        '{"createSurface":{"id":"surf-1","surfaceKind":"inline"}}',
        '{"updateComponents":{"surfaceId":"surf-1","components":[{"id":"t1","componentKind":"Text","text":"A2UI 你好"}]}}',
      ].join('\n'),
    );
    await el.updateComplete;

    expect(el.querySelector('.a2ui-surface--inline')).toBeTruthy();
    expect(el.textContent).toContain('A2UI 你好');
  });

  it('opens the session named by a deep link instead of the stored session', async () => {
    localStorage.setItem('tinyiothub_chat_last_session', 'agent:ws-1:default/stored');
    hoisted.fetchSessionEvents.mockResolvedValue({ events: [], highWatermark: 0 });
    const el = document.createElement('view-chat') as ChatView;
    el.initialSessionKey = 'agent:ws-1:default/linked';
    document.body.appendChild(el);
    await vi.waitFor(() => expect(hoisted.connectSessionEvents).toHaveBeenCalled());
    expect(hoisted.connectSessionEvents.mock.calls[0][0].sessionKey).toBe(
      'agent:ws-1:default/linked',
    );
    expect(localStorage.getItem('tinyiothub_chat_last_session')).toBe(
      'agent:ws-1:default/linked',
    );
  });

  it('changes the session permission mode', async () => {
    localStorage.setItem('tinyiothub_chat_last_session', 'agent:ws-1:default/s1');
    hoisted.listChatSessions.mockResolvedValue([
      { sessionKey: 'agent:ws-1:default/s1', kind: 'chat', status: 'active', label: '新会话', permissionMode: 'ask', updatedAt: 1 },
    ]);
    hoisted.updateSessionPermission.mockResolvedValue(undefined);
    const el = document.createElement('view-chat') as ChatView;
    document.body.appendChild(el);
    await vi.waitFor(() => expect(el.querySelector('.permission-mode')).toBeTruthy());
    const select = el.querySelector('.permission-mode') as HTMLSelectElement;
    select.value = 'read_only';
    select.dispatchEvent(new Event('change'));
    await el.updateComplete;
    expect(hoisted.updateSessionPermission).toHaveBeenCalledWith('agent:ws-1:default/s1', 'read_only');
  });

  it('ignores a stale permission rollback after switching sessions', async () => {
    localStorage.setItem('tinyiothub_chat_last_session', 'agent:ws-1:default/a');
    hoisted.listChatSessions.mockResolvedValue([
      { sessionKey: 'agent:ws-1:default/a', kind: 'chat', status: 'active', label: 'A', permissionMode: 'ask', updatedAt: 1 },
      { sessionKey: 'agent:ws-1:default/b', kind: 'chat', status: 'active', label: 'B', permissionMode: 'full_access', updatedAt: 2 },
    ]);
    hoisted.fetchSessionEvents.mockResolvedValue({ events: [], highWatermark: 0 });
    let rejectPut!: (error: Error) => void;
    hoisted.updateSessionPermission.mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectPut = reject;
      }),
    );
    const el = document.createElement('view-chat') as ChatView;
    document.body.appendChild(el);
    await vi.waitFor(() =>
      expect((el.querySelector('.permission-mode') as HTMLSelectElement)?.value).toBe('ask'),
    );

    const select = el.querySelector('.permission-mode') as HTMLSelectElement;
    select.value = 'read_only';
    select.dispatchEvent(new Event('change'));
    await el.updateComplete;
    expect(hoisted.updateSessionPermission).toHaveBeenCalledWith('agent:ws-1:default/a', 'read_only');

    el.querySelector('session-drawer')!.dispatchEvent(
      new CustomEvent('session-select', {
        detail: 'agent:ws-1:default/b',
        bubbles: true,
        composed: true,
      }),
    );
    await vi.waitFor(() =>
      expect((el.querySelector('.permission-mode') as HTMLSelectElement).value).toBe('full_access'),
    );

    rejectPut(new Error('保存权限模式失败'));
    await Promise.resolve();
    await el.updateComplete;

    expect((el.querySelector('.permission-mode') as HTMLSelectElement).value).toBe('full_access');
    expect(el.querySelector('.timeline-notice')).toBeNull();
  });

  it('hydrates a2ui history even when session events exist', async () => {
    hoisted.fetchSessionEvents.mockResolvedValue({ events: [turnStarted], highWatermark: 1 });
    const historyA2ui = [
      '{"createSurface":{"id":"hist-1","surfaceKind":"inline"}}',
      '{"updateComponents":{"surfaceId":"hist-1","components":[{"id":"t1","componentKind":"Text","text":"历史 A2UI"}]}}',
    ].join('\n');
    hoisted.loadChatHistory.mockImplementation(async (state: unknown) => {
      (state as { chatMessages: Array<Record<string, unknown>> }).chatMessages = [
        {
          role: 'assistant',
          content: [{ type: 'text', text: '历史消息' }],
          a2ui: historyA2ui,
        },
      ];
    });

    const el = document.createElement('view-chat') as ChatView;
    document.body.appendChild(el);
    await el.updateComplete;
    await vi.waitFor(() => expect(el.textContent).toContain('历史 A2UI'));

    expect(hoisted.loadChatHistory).toHaveBeenCalledTimes(1);
    expect(el.querySelector('.timeline')).toBeTruthy();
  });
});
