import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  fetchSessionEvents: vi.fn(),
  connectSessionEvents: vi.fn((_opts: { sessionKey: string; after: number }) => () => {}),
  fetchSessionTodos: vi.fn(),
  listChatSessions: vi.fn().mockResolvedValue([]),
  fetchAssistantHud: vi.fn(),
  getWorkspaceId: vi.fn(() => 'ws-1'),
}));
vi.mock('../../api/assistant.js', () => hoisted);
vi.mock('../../api/client.js', async () => {
  const actual = await vi.importActual<typeof import('../../api/client.js')>('../../api/client.js');
  return { ...actual, getWorkspaceId: hoisted.getWorkspaceId };
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
});
