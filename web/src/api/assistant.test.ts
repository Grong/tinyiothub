import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiDelete: vi.fn(),
  getAuthToken: vi.fn(() => 'tok'),
  getWorkspaceId: vi.fn(() => 'ws-1'),
}));

vi.mock('./client.js', async () => {
  const actual = await vi.importActual<typeof import('./client.js')>('./client.js');
  return {
    ...actual,
    ApiClient: {
      get: hoisted.apiGet,
      post: hoisted.apiPost,
      delete: hoisted.apiDelete,
    },
    getAuthToken: hoisted.getAuthToken,
    getWorkspaceId: hoisted.getWorkspaceId,
  };
});

import {
  answerQuestion,
  deleteChatSession,
  fetchAssistantHud,
  fetchSessionEvents,
  fetchSessionProjection,
  listChatSessions,
  renameChatSession,
  updateAssistantSchedule,
} from './assistant.js';

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('assistant api', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    hoisted.apiGet.mockReset();
    hoisted.apiPost.mockReset();
    hoisted.apiDelete.mockReset();
  });

  it('lists sessions through the ApiClient envelope', async () => {
    hoisted.apiGet.mockResolvedValue({
      code: 0,
      result: { sessions: [{ sessionKey: 'agent:ws-1:default/a', kind: 'chat', status: 'active', label: null }] },
    });
    const sessions = await listChatSessions('default');
    expect(hoisted.apiGet).toHaveBeenCalledWith('/chat/sessions', { limit: 100 });
    expect(sessions).toHaveLength(1);
    expect(sessions[0].sessionKey).toBe('agent:ws-1:default/a');
  });

  it('encodes the session key for rename and delete', async () => {
    hoisted.apiPost.mockResolvedValue({ code: 0, result: {} });
    hoisted.apiDelete.mockResolvedValue({ code: 0, result: {} });
    await renameChatSession('agent:ws-1:default/a b', '巡检');
    expect(hoisted.apiPost).toHaveBeenCalledWith(
      '/chat/sessions/agent%3Aws-1%3Adefault%2Fa%20b/label',
      { label: '巡检' },
    );
    await deleteChatSession('agent:ws-1:default/a b');
    expect(hoisted.apiDelete).toHaveBeenCalledWith('/chat/sessions/agent%3Aws-1%3Adefault%2Fa%20b');
  });

  it('fetches the durable session projection', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        turn: { turnId: 't1', phase: 'executing', agenda: [{ key: 'inspection', label: '巡检', status: 'in_progress' }], todos: [{ content: '检查设备', status: 'in_progress' }], lastSeq: 9 },
        highWatermark: 12,
      }),
    );
    const projection = await fetchSessionProjection('agent:ws-1:default/a');
    const [url] = fetchMock.mock.calls[0];
    expect(String(url)).toBe('/api/v1/chat/sessions/agent%3Aws-1%3Adefault%2Fa/projection');
    expect(projection.turn.agenda[0].key).toBe('inspection');
    expect(projection.turn.todos?.[0].content).toBe('检查设备');
    expect(projection.highWatermark).toBe(12);
  });

  it('fetches session events raw with cursor params', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ events: [{ seq: 3, eventId: 'e3', eventType: 'turn/start', payloadJson: {} }], highWatermark: 3 }),
    );
    const page = await fetchSessionEvents('agent:ws-1:default/a', 2, 100);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe('/api/v1/chat/sessions/agent%3Aws-1%3Adefault%2Fa/events?after=2&limit=100');
    expect((init as RequestInit).headers).toMatchObject({ Authorization: 'Bearer tok' });
    expect(page.highWatermark).toBe(3);
  });

  it('fetches complete turn pages with an older-turn cursor', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ events: [], highWatermark: 30, hasMore: true, nextBefore: 20 }),
    );
    await fetchSessionEvents('agent:ws-1:default/a', 0, 200, { turns: 5, before: 30 });
    expect(String(fetchMock.mock.calls[0][0])).toBe(
      '/api/v1/chat/sessions/agent%3Aws-1%3Adefault%2Fa/events?after=0&limit=200&turns=5&before=30',
    );
  });

  it('maps answer status codes to ApiError', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('', { status: 409 }));
    await expect(
      answerQuestion('agent:ws-1:default/a', 'q1', { answers: [{ questionIndex: 0, selected: ['全部'] }] }),
    ).rejects.toMatchObject({ status: 409 });
  });

  it('fetches the HUD raw and updates the schedule with camelCase keys', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(jsonResponse({ assistant: { status: 'idle' }, devices: { total: 0, online: 0, offline: 0 } }))
      .mockResolvedValueOnce(jsonResponse({ enabled: true, intervalSeconds: 600 }));
    const hud = await fetchAssistantHud('ws-1');
    expect(hud.assistant.status).toBe('idle');
    await updateAssistantSchedule('ws-1', { intervalSeconds: 600 });
    const [url, init] = fetchMock.mock.calls[1];
    expect(String(url)).toBe('/api/v1/workspaces/ws-1/agent/schedule');
    expect((init as RequestInit).method).toBe('PUT');
    expect(JSON.parse(String((init as RequestInit).body))).toEqual({ intervalSeconds: 600 });
  });
});
