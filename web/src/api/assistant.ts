//! Plan 1/2 原始 JSON 端点客户端（无 ApiResponse 信封）。
//!
//! - 会话列表/改名/删除仍走 ApiClient（信封 + camelCase 自动转换）。
//! - 事件/待答/答题/HUD/Schedule 是裸 JSON，直接用 fetch + buildUrl。
//! - SessionKey 必须 encodeURIComponent（包含 ':' 与 '/'）。

import { ApiClient, ApiError, buildUrl, getAuthToken } from './client.js';

export interface SessionEvent {
  seq: number;
  eventId: string;
  sessionId: string;
  executionMode: string;
  runId: string | null;
  taskId: string | null;
  eventType: string;
  occurredAt: string;
  causeId: number | null;
  schemaV: number;
  payloadDigest: string;
  payloadJson: Record<string, unknown>;
}

export interface SessionEventPage {
  events: SessionEvent[];
  highWatermark: number;
}

export type TodoStatus = 'pending' | 'in_progress' | 'completed';

export interface TodoItem {
  content: string;
  status: TodoStatus;
  activeForm: string;
}

export interface QuestionOption {
  label: string;
  description: string;
}

export interface Question {
  question: string;
  header: string;
  options: QuestionOption[];
  multiSelect: boolean;
}

export interface QuestionAnswer {
  answers: Array<{
    questionIndex: number;
    selected: string[];
    text?: string | null;
  }>;
}

export interface SessionSummary {
  sessionKey: string;
  workspaceId: string;
  agentId: string;
  label: string | null;
  kind: 'chat' | 'scheduled' | 'event' | string;
  status: 'active' | 'ended' | string;
  createdAt: number;
  updatedAt: number;
}

export interface HudSnapshot {
  assistant: { status: 'idle' | 'working' | 'waiting_for_user' | 'paused' | 'error'; lastOutcome: string | null };
  devices: { total: number; online: number; offline: number };
  alarms: { highPriority: number };
  pending: { questions: number; approvals: number };
  activity: { activeSessions: number };
  schedule: ScheduleSnapshot;
  recentEvents: SessionEvent[];
}

export interface ScheduleSnapshot {
  enabled: boolean;
  intervalSeconds: number;
  prompt: string;
  nextWakeAt: string | null;
  lastWakeAt: string | null;
  lastOutcome: string | null;
}

function authHeaders(extra?: Record<string, string>): Record<string, string> {
  const headers: Record<string, string> = { Accept: 'application/json', ...extra };
  const token = getAuthToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

function sessionPath(sessionKey: string, suffix = ''): string {
  return `/chat/sessions/${encodeURIComponent(sessionKey)}${suffix}`;
}

// 裸 JSON 端点的错误直接以 HTTP status 表达，同时保留 ApiError 的 code 语义。
function httpError(status: number, message: string): ApiError {
  return Object.assign(new ApiError(status, message), { status });
}

async function rawJson<T>(response: Response, message: string): Promise<T> {
  if (!response.ok) {
    throw httpError(response.status, message);
  }
  return (await response.json()) as T;
}

export async function listChatSessions(agentId: string): Promise<SessionSummary[]> {
  const response = await ApiClient.get<{ sessions: SessionSummary[] }>('/chat/sessions', {
    agent_id: agentId,
    limit: 50,
  });
  return response.result?.sessions ?? [];
}

export async function renameChatSession(sessionKey: string, label: string): Promise<void> {
  await ApiClient.post(sessionPath(sessionKey, '/label'), { label });
}

export async function deleteChatSession(sessionKey: string): Promise<void> {
  await ApiClient.delete(sessionPath(sessionKey));
}

export async function fetchSessionEvents(
  sessionKey: string,
  after = 0,
  limit = 200,
): Promise<SessionEventPage> {
  const response = await fetch(buildUrl(`${sessionPath(sessionKey, '/events')}?after=${after}&limit=${limit}`), {
    headers: authHeaders(),
    credentials: 'include',
  });
  return rawJson<SessionEventPage>(response, '加载会话事件失败');
}

export async function fetchSessionTodos(sessionKey: string): Promise<TodoItem[]> {
  const response = await fetch(buildUrl(sessionPath(sessionKey, '/todos')), {
    headers: authHeaders(),
    credentials: 'include',
  });
  return rawJson<TodoItem[]>(response, '加载任务清单失败');
}

export async function answerQuestion(
  sessionKey: string,
  questionId: string,
  answer: QuestionAnswer,
): Promise<void> {
  const response = await fetch(
    buildUrl(sessionPath(sessionKey, `/questions/${encodeURIComponent(questionId)}/answer`)),
    {
      method: 'POST',
      headers: authHeaders({ 'Content-Type': 'application/json' }),
      credentials: 'include',
      body: JSON.stringify(answer),
    },
  );
  if (response.status === 204) return;
  if (response.status === 404) throw httpError(404, '问题不存在或会话已结束');
  if (response.status === 403) throw httpError(403, '问题属于其他会话');
  if (response.status === 409) throw httpError(409, '问题已关闭');
  throw httpError(response.status, '提交回答失败');
}

export async function fetchAssistantHud(workspaceId: string): Promise<HudSnapshot> {
  const response = await fetch(buildUrl(`/workspaces/${encodeURIComponent(workspaceId)}/agent/hud`), {
    headers: authHeaders(),
    credentials: 'include',
  });
  return rawJson<HudSnapshot>(response, '加载全局状态失败');
}

export async function fetchAssistantSchedule(workspaceId: string): Promise<ScheduleSnapshot> {
  const response = await fetch(
    buildUrl(`/workspaces/${encodeURIComponent(workspaceId)}/agent/schedule`),
    { headers: authHeaders(), credentials: 'include' },
  );
  return rawJson<ScheduleSnapshot>(response, '加载调度失败');
}

export async function updateAssistantSchedule(
  workspaceId: string,
  patch: { enabled?: boolean; intervalSeconds?: number; prompt?: string },
): Promise<ScheduleSnapshot> {
  const response = await fetch(
    buildUrl(`/workspaces/${encodeURIComponent(workspaceId)}/agent/schedule`),
    {
      method: 'PUT',
      headers: authHeaders({ 'Content-Type': 'application/json' }),
      credentials: 'include',
      body: JSON.stringify(patch),
    },
  );
  return rawJson<ScheduleSnapshot>(response, '保存调度失败');
}

/**
 * 游标式 session_events SSE：断线后从最后 seq 继续，退避 500ms→5s。
 * 返回关闭函数。
 */
export function connectSessionEvents(opts: {
  sessionKey: string;
  after: number;
  onEvent: (event: SessionEvent) => void;
  onError?: (error: Error) => void;
  signal?: AbortSignal;
}): () => void {
  const controller = new AbortController();
  let closed = false;
  let cursor = opts.after;

  const abort = () => {
    closed = true;
    controller.abort();
  };
  opts.signal?.addEventListener('abort', abort, { once: true });

  async function run() {
    let backoff = 500;
    while (!closed) {
      try {
        const response = await fetch(
          buildUrl(`${sessionPath(opts.sessionKey, '/events/stream')}?after=${cursor}`),
          {
            headers: authHeaders({ Accept: 'text/event-stream' }),
            credentials: 'include',
            signal: controller.signal,
          },
        );
        if (!response.ok || !response.body) {
          throw new Error(`SSE connection failed: HTTP ${response.status}`);
        }
        backoff = 500;
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let data = '';
        let eventType = 'message';

        while (!closed) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split('\n');
          buffer = lines.pop() ?? '';
          for (const rawLine of lines) {
            const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine;
            if (line === '') {
              if (data) {
                if (eventType === 'session' || eventType === 'message') {
                  try {
                    const event = JSON.parse(data) as SessionEvent;
                    cursor = Math.max(cursor, event.seq);
                    opts.onEvent(event);
                  } catch (error) {
                    opts.onError?.(error as Error);
                  }
                }
              }
              data = '';
              eventType = 'message';
              continue;
            }
            if (line.startsWith(':')) continue;
            if (line.startsWith('event:')) {
              eventType = line.slice(6).trim();
              continue;
            }
            if (line.startsWith('data:')) {
              const chunk = line.slice(5).replace(/^ /, '');
              data = data ? `${data}\n${chunk}` : chunk;
            }
          }
        }
      } catch (error) {
        if ((error as Error).name === 'AbortError' || closed) break;
        opts.onError?.(error as Error);
      }
      if (!closed) {
        await new Promise((resolve) => setTimeout(resolve, backoff));
        backoff = Math.min(backoff * 2, 5000);
      }
    }
  }

  void run();
  return abort;
}
