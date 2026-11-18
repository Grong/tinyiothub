# Assistant Codex-Style Frontend Implementation Plan (Plan 3 / 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把现有聊天页改造成 Codex 式单列工作台：左侧会话抽屉（chat/scheduled/event 分组）、居中会话时间线（turn/tool/todo/提问/子代理/skill 全部来自 `session_events`）、就地答题卡片、顶部全局 HUD 浮动卡与详情面板。

**Architecture:** 新增 `web/src/api/assistant.ts` 统一封装 Plan 1/2 的原始 JSON 端点（session events、todos、questions、sessions、hud、schedule）与游标式 SSE；新增纯函数时间线模型 `timeline-model.ts` 把事件折叠为可渲染块；`views/chat.ts` 改为「抽屉 + 时间线 + composer」结构，文本增量仍走现有 `/chat/stream` 的 `ChatEvent`，过程展示改由 `session_events` 驱动；Todo/提问/子代理/工具全部内联在时间线中，HUD 独立轮询。

**Tech Stack:** Vite 7、Lit 3、TypeScript 5.7、`@lit-labs/signals`（可选）、vitest 4 + jsdom、现有 `md()`（marked + DOMPurify）与设计系统 tokens。**Node 24 必需**（默认 Node 18 下 vite/vitest 会失败）。

## Plan Boundary

本计划是 4 份计划中的第 3 份：

- **Plan 1（已完成）**：后端 Session Loop、三个原语、`session_events`、事件/todos/答题 API、聊天接线。
- **Plan 2（已完成）**：Schedule、TriggerRouter、WakeRunner、HUD/ Schedule 后端、自主权限门与关停取消。
- **Plan 3（本文件）**：Codex 式前端。
- **Plan 4**：旧代码清理与命名收敛（含旧 chat 渲染、旧触发器、权限模式强制接线）。

明确不在本计划内：

- **权限模式接线（read_only / ask / full_access）**：后端尚未按 Session 强制；本计划只在 composer 显示只读徽标「需要审批」，不提供可切换的假控件。真正接线与强制在 Plan 4。
- **审批卡片**：`approval.requested/resolved` 尚未进入 `session_events`（Plan 1 延后），本计划不伪造；`ask_user` 卡片是唯一阻塞式用户输入。
- **附件上传、模型切换**：现有 composer 均无可用后端契约，不做。
- **A2UI 渲染改造**：保留现有 `A2uiRendererEngine` 与历史消息里的 surface 渲染；新时间线不重写 A2UI。
- **删除旧组件**：`grouped-render.ts` 等旧渲染在 Plan 4 清理；本计划停止在聊天页使用它们，但不删除文件。

---

## Global Constraints

以下约束来自 `docs/superpowers/specs/2026-11-07-assistant-harness-design.md` §10 与项目约定，每个 Task 都隐含包含：

- **版式**：单列居中，最大宽度约 **760px**；助手正文文档式，用户消息右侧胶囊；会话列表在左侧抽屉（对话/巡检/事件分组），不是常驻栏。
- **过程可见**：Todo 内联在当轮顶部并显示 `计划中 x/y`；工具调用默认一行摘要，点击展开参数与输出；`ask_user` 与审批都是内联阻塞卡片（本期只做 `ask_user`）；子代理是内联卡片，可展开。
- **HUD**：顶部浮动卡显示 Assistant 状态（`idle|working|waiting_for_user|paused|error`）、设备总数/在线/离线、高优告警、待答/待批、活动会话、下次唤醒；点击展开详情；只在 Assistant/对话主区域显示。
- **文案**：中文界面；错误文案简短可行动；不使用 toast 代替状态。
- **样式**：沿用 `web/DESIGN_SYSTEM.md` 与 `src/styles/base.css` tokens（`--bg/--card/--border/--accent/--radius-*/--space-*`）；深色为主、跟随系统；不做 KPI 卡墙、不做假进度。
- **数据**：过程展示只来自 `session_events`；文本增量来自现有 `ChatEvent`；不要新增后端端点。
- **原始 JSON 端点**（`/chat/sessions/{key}/events[/stream]`、`/todos`、`/questions/{id}/answer`、`/agent/hud`、`/agent/schedule`）**不走 `ApiClient` 信封**，用 `fetch` + `buildUrl`；会话列表/改名/删除走 `ApiClient`。
- **SessionKey**：前端保持 `agent:<workspaceId>:<agentId>/<uuid>`；URL 中必须 `encodeURIComponent`（`/` → `%2F`）。
- **Node 24**：所有 web 命令前先 `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`。
- **TDD**：每个 Task 先写失败测试，再实现，再跑测试，再提交。
- **提交**：每个 Task 一个 commit；不要提交无关工作区文件（`docs/superpowers/specs/2026-11-02-agent-harness-core-design.md`、`docs/superpowers/plans/2026-11-03-agent-harness-core.md` 不要动）。

---

## File Structure

### 新增

- `web/src/api/assistant.ts` — session events / todos / questions / sessions / hud / schedule 客户端。
- `web/src/ui/chat/timeline-model.ts` — 纯函数时间线模型（事件折叠为块）。
- `web/src/ui/chat/timeline-render.ts` — 时间线渲染函数（turn/tool/todo/subagent/skill/message）。
- `web/src/ui/chat/question-card.ts` — `<question-card>` 交互组件。
- `web/src/ui/chat/session-drawer.ts` — `<session-drawer>` 会话抽屉。
- `web/src/ui/components/global-hud.ts` — `<global-hud>` 浮动 HUD + 详情。
- `web/src/styles/chat/codex.css` — Codex 版式/时间线/HUD 样式。

### 修改

- `web/src/ui/views/chat.ts` — 结构改为抽屉 + 时间线 + composer；接入 SSE/事件/提问/HUD。
- `web/src/styles/chat.css` — import `codex.css`。
- `web/src/ui/app.ts` — 聊天路由使用 `.content--chat` 容器（现有类未被使用）。

---

### Task 1: Assistant API 客户端

**Files:**

- Create: `web/src/api/assistant.ts`
- Test: `web/src/api/assistant.test.ts`

**Interfaces:**

- Consumes: `buildUrl`, `ApiClient`（`web/src/api/client.ts`）、`getAuthToken`。
- Produces:
  - 类型 `SessionEvent`、`SessionEventPage`、`TodoItem`、`Question`、`QuestionAnswer`、`SessionSummary`、`HudSnapshot`、`ScheduleSnapshot`。
  - `listChatSessions(agentId: string): Promise<SessionSummary[]>`
  - `renameChatSession(sessionKey: string, label: string): Promise<void>`
  - `deleteChatSession(sessionKey: string): Promise<void>`
  - `fetchSessionEvents(sessionKey: string, after?: number, limit?: number): Promise<SessionEventPage>`
  - `connectSessionEvents(opts: { sessionKey: string; after: number; onEvent: (event: SessionEvent) => void; onError?: (error: Error) => void; signal?: AbortSignal }): () => void`
  - `fetchSessionTodos(sessionKey: string): Promise<TodoItem[]>`
  - `answerQuestion(sessionKey: string, questionId: string, answer: QuestionAnswer): Promise<void>`
  - `fetchAssistantHud(workspaceId: string): Promise<HudSnapshot>`
  - `fetchAssistantSchedule(workspaceId: string): Promise<ScheduleSnapshot>`
  - `updateAssistantSchedule(workspaceId: string, patch: { enabled?: boolean; intervalSeconds?: number; prompt?: string }): Promise<ScheduleSnapshot>`

- [ ] **Step 1: 写失败测试**

`web/src/api/assistant.test.ts`：

```ts
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
    expect(hoisted.apiGet).toHaveBeenCalledWith('/chat/sessions', { agent_id: 'default', limit: 50 });
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

  it('fetches session events raw with cursor params', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ events: [{ seq: 3, eventId: 'e3', eventType: 'turn.started', payloadJson: {} }], highWatermark: 3 }),
    );
    const page = await fetchSessionEvents('agent:ws-1:default/a', 2, 100);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe('/api/v1/chat/sessions/agent%3Aws-1%3Adefault%2Fa/events?after=2&limit=100');
    expect((init as RequestInit).headers).toMatchObject({ Authorization: 'Bearer tok' });
    expect(page.highWatermark).toBe(3);
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
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/api/assistant.test.ts
```

Expected: FAIL，模块 `./assistant.js` 不存在。

- [ ] **Step 3: 写最小实现**

`web/src/api/assistant.ts`：

```ts
//! Plan 1/2 原始 JSON 端点客户端（无 ApiResponse 信封）。
//!
//! - 会话列表/改名/删除仍走 ApiClient（信封 + camelCase 自动转换）。
//! - 事件/待答/答题/HUD/Schedule 是裸 JSON，直接用 fetch + buildUrl。
//! - SessionKey 必须 encodeURIComponent（包含 ':' 与 '/'）。

import { ApiClient, ApiError, buildUrl, getAuthToken, getWorkspaceId } from './client.js';

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

function sessionPath(sessionKey: string, suffix: string): string {
  return `/chat/sessions/${encodeURIComponent(sessionKey)}${suffix}`;
}

async function rawJson<T>(response: Response, message: string): Promise<T> {
  if (!response.ok) {
    throw new ApiError(response.status, message);
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
  if (response.status === 404) throw new ApiError(404, '问题不存在或会话已结束');
  if (response.status === 403) throw new ApiError(403, '问题属于其他会话');
  if (response.status === 409) throw new ApiError(409, '问题已关闭');
  throw new ApiError(response.status, '提交回答失败');
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
```

> `ApiError` 需要从 `client.ts` 导出；若当前未导出，在本 Task 中把它改为 `export class ApiError`。`ApiClient.delete` 是否存在同名方法以源码为准（`request({method:'DELETE'})` 封装）。

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/api/assistant.test.ts
npm run type-check
```

Expected: 测试 PASS；type-check 通过。

- [ ] **Step 5: 提交**

```bash
git add web/src/api/assistant.ts web/src/api/assistant.test.ts web/src/api/client.ts
git commit -m "feat(web): add assistant session/hud api client"
```

（若未改 `client.ts`，从 `git add` 去掉。）

---

### Task 2: 时间线模型

**Files:**

- Create: `web/src/ui/chat/timeline-model.ts`
- Test: `web/src/ui/chat/timeline-model.test.ts`

**Interfaces:**

- Consumes: `SessionEvent`、`TodoItem`、`Question`。
- Produces:
  - `type TimelineBlock`（`user|assistant|turn|tool|skill|subagent|notice`）
  - `interface PendingQuestion { questionId: string; questions: Question[] }`
  - `class TimelineModel`：`ingest(page: SessionEventPage): void`、`ingestEvent(event: SessionEvent): void`、`blocks: TimelineBlock[]`、`todos: TodoItem[]`、`pendingQuestion: PendingQuestion | null`、`cursor: number`、`highWatermark: number`、`firstUserText: string | null`
  - `timelineFromHistory(messages): TimelineBlock[]`（历史兜底）

- [ ] **Step 1: 写失败测试**

`timeline-model.test.ts`：

```ts
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
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/timeline-model.test.ts
```

Expected: FAIL，模块不存在。

- [ ] **Step 3: 写最小实现**

`web/src/ui/chat/timeline-model.ts`：

```ts
import type { Question, SessionEvent, SessionEventPage, TodoItem } from '../../api/assistant.js';

export type TimelineBlock =
  | { kind: 'user'; id: string; text: string; at: string }
  | { kind: 'assistant'; id: string; text: string; at: string }
  | { kind: 'turn'; id: string; status: 'running' | 'completed' | 'failed' | 'cancelled'; at: string }
  | {
      kind: 'tool';
      id: string;
      name: string;
      args: unknown;
      result?: string;
      evidence?: unknown[];
      at: string;
    }
  | { kind: 'skill'; id: string; name: string; version: string; at: string }
  | {
      kind: 'subagent';
      id: string;
      description: string;
      subagentType?: string;
      status?: string;
      summary?: string;
      at: string;
    }
  | {
      kind: 'notice';
      id: string;
      tone: 'answered' | 'timed_out' | 'cancelled';
      text: string;
      at: string;
    };

export interface PendingQuestion {
  questionId: string;
  questions: Question[];
}

function asString(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
}

export class TimelineModel {
  blocks: TimelineBlock[] = [];
  todos: TodoItem[] = [];
  pendingQuestion: PendingQuestion | null = null;
  cursor = 0;
  highWatermark = 0;
  firstUserText: string | null = null;

  private seen = new Set<string>();

  reset(): void {
    this.blocks = [];
    this.todos = [];
    this.pendingQuestion = null;
    this.cursor = 0;
    this.highWatermark = 0;
    this.firstUserText = null;
    this.seen.clear();
  }

  ingest(page: SessionEventPage): void {
    this.highWatermark = Math.max(this.highWatermark, page.highWatermark);
    const ordered = [...page.events].sort((a, b) => a.seq - b.seq);
    for (const event of ordered) this.ingestEvent(event);
  }

  ingestEvent(event: SessionEvent): void {
    if (this.seen.has(event.eventId)) return;
    this.seen.add(event.eventId);
    this.cursor = Math.max(this.cursor, event.seq);
    const payload = event.payloadJson ?? {};
    const at = event.occurredAt;

    switch (event.eventType) {
      case 'turn.started':
        this.blocks.push({ kind: 'turn', id: asString(payload.turnId) || event.eventId, status: 'running', at });
        break;
      case 'turn.completed': {
        const id = asString(payload.turnId);
        const status = asString(payload.outcome);
        const block = this.blocks.find((item) => item.kind === 'turn' && item.id === id);
        const mapped = status === 'completed' ? 'completed' : status === 'failed' ? 'failed' : 'cancelled';
        if (block && block.kind === 'turn') block.status = mapped;
        else this.blocks.push({ kind: 'turn', id: id || event.eventId, status: mapped, at });
        break;
      }
      case 'message.created': {
        const role = asString(payload.role);
        const text = asString(payload.text);
        if (!text) break;
        const id = asString(payload.messageId) || event.eventId;
        if (role === 'user') {
          this.blocks.push({ kind: 'user', id, text, at });
          if (!this.firstUserText) this.firstUserText = text;
        } else {
          this.blocks.push({ kind: 'assistant', id, text, at });
        }
        break;
      }
      case 'tool.called': {
        const id = asString(payload.toolCallId) || event.eventId;
        const name = asString(payload.tool);
        if (name === 'canvas') break;
        this.blocks.push({ kind: 'tool', id, name, args: payload.args ?? null, at });
        break;
      }
      case 'tool.returned': {
        const id = asString(payload.toolCallId) || event.eventId;
        const name = asString(payload.tool);
        if (name === 'canvas') break;
        const result = asString(payload.result);
        const evidence = Array.isArray(payload.evidence) ? payload.evidence : undefined;
        const block = this.blocks.find((item) => item.kind === 'tool' && item.id === id);
        if (block && block.kind === 'tool') {
          block.result = result;
          block.evidence = evidence;
        } else {
          this.blocks.push({ kind: 'tool', id, name, args: null, result, evidence, at });
        }
        break;
      }
      case 'skill.loaded':
        this.blocks.push({
          kind: 'skill',
          id: asString(payload.skillId) || event.eventId,
          name: asString(payload.name),
          version: asString(payload.version),
          at,
        });
        break;
      case 'todo.updated':
        if (Array.isArray(payload.todos)) this.todos = payload.todos as TodoItem[];
        break;
      case 'question.asked':
        this.pendingQuestion = {
          questionId: asString(payload.questionId),
          questions: Array.isArray(payload.questions) ? (payload.questions as Question[]) : [],
        };
        break;
      case 'question.answered':
        if (this.pendingQuestion?.questionId === asString(payload.questionId)) {
          this.pendingQuestion = null;
        }
        this.blocks.push({
          kind: 'notice',
          id: asString(payload.questionId) || event.eventId,
          tone: payload.timedOut ? 'timed_out' : 'answered',
          text: payload.timedOut ? '等待回答超时' : '已提交回答',
          at,
        });
        break;
      case 'subagent.started': {
        this.blocks.push({
          kind: 'subagent',
          id: asString(payload.subagentId) || event.eventId,
          description: asString(payload.description),
          subagentType: asString(payload.subagentType),
          status: 'running',
          at,
        });
        break;
      }
      case 'subagent.completed': {
        const id = asString(payload.subagentId);
        const block = this.blocks.find((item) => item.kind === 'subagent' && item.id === id);
        const status = asString(payload.status);
        const summary = asString(payload.summary);
        if (block && block.kind === 'subagent') {
          block.status = status;
          block.summary = summary;
        } else {
          this.blocks.push({ kind: 'subagent', id: id || event.eventId, description: id, status, summary, at });
        }
        break;
      }
      case 'session.ended':
        this.blocks.push({
          kind: 'notice',
          id: event.eventId,
          tone: 'cancelled',
          text: `会话结束：${asString(payload.outcome) || 'unknown'}`,
          at,
        });
        break;
      default:
        break;
    }
  }
}

export function timelineFromHistory(
  messages: Array<{ role: string; content?: Array<{ type: string; text?: string }> }>,
): TimelineBlock[] {
  const blocks: TimelineBlock[] = [];
  let index = 0;
  for (const message of messages) {
    if (message.role !== 'user' && message.role !== 'assistant') continue;
    const text = (message.content ?? [])
      .filter((block) => block.type === 'text' && block.text)
      .map((block) => block.text as string)
      .join('\n');
    if (!text) continue;
    index += 1;
    blocks.push({
      kind: message.role === 'user' ? 'user' : 'assistant',
      id: `history-${index}`,
      text,
      at: '',
    });
  }
  return blocks;
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/timeline-model.test.ts
npm run type-check
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/ui/chat/timeline-model.ts web/src/ui/chat/timeline-model.test.ts
git commit -m "feat(web): add session timeline model"
```

---

### Task 3: 时间线渲染（turn / message / tool / skill / subagent / todo）

**Files:**

- Create: `web/src/ui/chat/timeline-render.ts`
- Create: `web/src/styles/chat/codex.css`
- Modify: `web/src/styles/chat.css`（import `codex.css`）
- Test: `web/src/ui/chat/timeline-render.test.ts`

**Interfaces:**

- Consumes: `TimelineBlock`、`TodoItem`、`md()`（`web/src/ui/shared/markdown.ts`）。
- Produces:
  - `renderTimeline(blocks: TimelineBlock[], opts?: { liveText?: string; sending?: boolean }): TemplateResult`
  - `renderTodoPanel(todos: TodoItem[]): TemplateResult | typeof nothing`
  - `todoProgress(todos: TodoItem[]): { done: number; total: number }`
  - `toolSummary(block): string`
  - `renderQuestionCard(...)` 不在此 Task（Task 4）。

- [ ] **Step 1: 写失败测试**

`timeline-render.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { todoProgress, toolSummary, summarizeTodos } from './timeline-render.js';

describe('timeline render helpers', () => {
  it('computes todo progress', () => {
    expect(todoProgress([])).toEqual({ done: 0, total: 0 });
    expect(
      todoProgress([
        { content: 'a', status: 'completed', activeForm: 'a' },
        { content: 'b', status: 'in_progress', activeForm: 'b' },
        { content: 'c', status: 'pending', activeForm: 'c' },
      ]),
    ).toEqual({ done: 1, total: 3 });
  });

  it('summarizes tools as name(object) -> result', () => {
    expect(
      toolSummary({ kind: 'tool', id: 'c1', name: 'search_things', args: { keyword: '3号楼' }, at: '' }),
    ).toBe('search_things(3号楼)');
    expect(
      toolSummary({ kind: 'tool', id: 'c1', name: 'search_things', args: {}, result: '12 台设备', at: '' }),
    ).toBe('search_things() → 12 台设备');
  });

  it('summarizes todos for the collapsed line', () => {
    expect(
      summarizeTodos([
        { content: 'a', status: 'completed', activeForm: 'a' },
        { content: 'b', status: 'in_progress', activeForm: 'b' },
      ]),
    ).toBe('计划中 1/2');
  });
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/timeline-render.test.ts
```

Expected: FAIL，模块不存在。

- [ ] **Step 3: 写最小实现**

`web/src/ui/chat/timeline-render.ts`：

```ts
import { html, nothing, type TemplateResult } from 'lit';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import { md } from '../shared/markdown.js';
import type { TimelineBlock } from './timeline-model.js';
import type { TodoItem } from '../../api/assistant.js';

const TOOL_ICONS: Record<string, string> = {
  get_thing: '◎',
  search_things: '⌕',
  read_properties: '◫',
  write_properties: '✎',
  send_command: '▸',
  get_device_history: '◷',
  list_alarms: '⚠',
  acknowledge_alarm: '✓',
  resolve_alarm: '✓',
  query_data: '⌕',
  aggregate_data: 'Σ',
  todo_write: '☑',
  ask_user: '?',
  spawn_subagent: '⧉',
};

function firstValue(args: unknown): string {
  if (!args || typeof args !== 'object') return '';
  const record = args as Record<string, unknown>;
  for (const key of ['keyword', 'thing_id', 'device_id', 'query', 'q', 'summary', 'description']) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  const first = Object.values(record).find((value) => typeof value === 'string' && value.trim());
  return typeof first === 'string' ? first.trim() : '';
}

export function toolSummary(block: Extract<TimelineBlock, { kind: 'tool' }>): string {
  const arg = firstValue(block.args);
  const head = `${block.name}(${arg})`;
  if (!block.result) return head;
  const result = block.result.replace(/\s+/g, ' ').trim().slice(0, 60);
  return `${head} → ${result}`;
}

export function todoProgress(todos: TodoItem[]): { done: number; total: number } {
  return {
    done: todos.filter((todo) => todo.status === 'completed').length,
    total: todos.length,
  };
}

export function summarizeTodos(todos: TodoItem[]): string {
  const { done, total } = todoProgress(todos);
  return `计划中 ${done}/${total}`;
}

export function renderTodoPanel(todos: TodoItem[]): TemplateResult | typeof nothing {
  if (todos.length === 0) return nothing;
  return html`
    <section class="timeline-todos" aria-label="任务清单">
      <div class="timeline-todos__head">${summarizeTodos(todos)}</div>
      <ol class="timeline-todos__list">
        ${todos.map(
          (todo) => html`
            <li class="timeline-todo timeline-todo--${todo.status}">
              <span class="timeline-todo__mark" aria-hidden="true">
                ${todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '▣' : '○'}
              </span>
              <span class="timeline-todo__text">${todo.content}</span>
            </li>
          `,
        )}
      </ol>
    </section>
  `;
}

function renderTool(block: Extract<TimelineBlock, { kind: 'tool' }>): TemplateResult {
  return html`
    <details class="timeline-tool">
      <summary class="timeline-tool__summary">
        <span class="timeline-tool__icon" aria-hidden="true">${TOOL_ICONS[block.name] ?? '⚙'}</span>
        <span class="timeline-tool__text">${toolSummary(block)}</span>
        <span class="timeline-tool__state ${block.result ? 'timeline-tool__state--ok' : 'timeline-tool__state--pending'}"
          >${block.result ? '完成' : '执行中'}</span
        >
      </summary>
      <div class="timeline-tool__body">
        <div class="timeline-tool__label">参数</div>
        <pre class="timeline-tool__pre">${JSON.stringify(block.args ?? {}, null, 2)}</pre>
        ${block.result
          ? html`
              <div class="timeline-tool__label">结果</div>
              <pre class="timeline-tool__pre">${block.result}</pre>
            `
          : nothing}
      </div>
    </details>
  `;
}

function renderSubagent(block: Extract<TimelineBlock, { kind: 'subagent' }>): TemplateResult {
  const status = block.status ?? 'running';
  return html`
    <details class="timeline-subagent">
      <summary class="timeline-subagent__summary">
        <span class="timeline-subagent__icon" aria-hidden="true">⧉</span>
        <span class="timeline-subagent__title">子代理：${block.description || block.id}</span>
        <span class="timeline-subagent__status timeline-subagent__status--${status}">${status}</span>
      </summary>
      <div class="timeline-subagent__body">
        ${block.subagentType ? html`<div class="timeline-subagent__meta">类型：${block.subagentType}</div>` : nothing}
        ${block.summary
          ? html`<div class="timeline-subagent__summary-text">${unsafeHTML(md(block.summary))}</div>`
          : nothing}
      </div>
    </details>
  `;
}

export function renderTimeline(
  blocks: TimelineBlock[],
  opts: { liveText?: string; sending?: boolean } = {},
): TemplateResult {
  const { liveText = '', sending = false } = opts;
  return html`
    <div class="timeline">
      ${blocks.map((block) => {
        switch (block.kind) {
          case 'user':
            return html`<div class="timeline-user"><div class="timeline-user__bubble">${block.text}</div></div>`;
          case 'assistant':
            return html`<article class="timeline-assistant">${unsafeHTML(md(block.text))}</article>`;
          case 'turn':
            return html`<div class="timeline-turn timeline-turn--${block.status}" aria-hidden="true"></div>`;
          case 'tool':
            return renderTool(block);
          case 'skill':
            return html`<div class="timeline-skill">已加载「${block.name}」技能${block.version ? ` · ${block.version}` : ''}</div>`;
          case 'subagent':
            return renderSubagent(block);
          case 'notice':
            return html`<div class="timeline-notice timeline-notice--${block.tone}">${block.text}</div>`;
          default:
            return nothing;
        }
      })}
      ${liveText
        ? html`<article class="timeline-assistant timeline-assistant--live">${unsafeHTML(md(liveText))}</article>`
        : nothing}
      ${sending && !liveText ? html`<div class="timeline-dots" aria-label="正在思考"><span></span><span></span><span></span></div>` : nothing}
    </div>
  `;
}
```

`web/src/styles/chat/codex.css`：

```css
/* Codex 式单列时间线（Plan 3） */
.codex-layout { display: grid; grid-template-columns: 240px minmax(0, 1fr); gap: var(--space-4); height: 100%; min-height: 0; }
.codex-main { display: flex; flex-direction: column; min-width: 0; min-height: 0; position: relative; }
.codex-thread { flex: 1; overflow-y: auto; padding: var(--space-4) var(--space-2) 140px; }
.timeline { width: 100%; max-width: 760px; margin: 0 auto; display: flex; flex-direction: column; gap: 10px; }
.timeline-user { display: flex; justify-content: flex-end; }
.timeline-user__bubble { max-width: 78%; background: var(--accent); color: #fff; padding: 10px 14px; border-radius: var(--radius-lg); white-space: pre-wrap; }
.timeline-assistant { color: var(--text); line-height: 1.65; font-size: 14.5px; }
.timeline-assistant :is(p, ul, ol, pre, blockquote) { margin: 0 0 10px; }
.timeline-assistant pre { background: var(--card); border: 1px solid var(--border); border-radius: var(--radius-md); padding: 10px; overflow-x: auto; }
.timeline-turn { height: 1px; background: var(--border); margin: 8px 0; }
.timeline-skill { font-size: 12px; color: var(--muted); padding-left: 2px; }
.timeline-tool { border: 1px solid var(--border); background: var(--card); border-radius: var(--radius-md); }
.timeline-tool__summary { display: flex; align-items: center; gap: 8px; padding: 8px 10px; cursor: pointer; list-style: none; }
.timeline-tool__summary::-webkit-details-marker { display: none; }
.timeline-tool__text { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: 12.5px; }
.timeline-tool__state { font-size: 11px; color: var(--muted); }
.timeline-tool__state--ok { color: var(--ok, #22c55e); }
.timeline-tool__body { border-top: 1px solid var(--border); padding: 10px; }
.timeline-tool__label { font-size: 11px; color: var(--muted); margin: 6px 0 4px; }
.timeline-tool__pre { margin: 0; max-height: 260px; overflow: auto; font-size: 12px; background: var(--panel-strong); padding: 8px; border-radius: var(--radius-sm); }
.timeline-subagent { border: 1px dashed var(--border); border-radius: var(--radius-md); padding: 8px 10px; }
.timeline-subagent__summary { display: flex; gap: 8px; align-items: center; cursor: pointer; list-style: none; }
.timeline-subagent__summary::-webkit-details-marker { display: none; }
.timeline-subagent__title { flex: 1; font-size: 13px; }
.timeline-subagent__status { font-size: 11px; color: var(--muted); }
.timeline-subagent__status--completed { color: var(--ok, #22c55e); }
.timeline-subagent__status--failed { color: var(--danger, #ef4444); }
.timeline-subagent__body { margin-top: 8px; font-size: 13px; color: var(--muted); }
.timeline-todos { width: 100%; max-width: 760px; margin: 0 auto 10px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--card); padding: 10px 12px; }
.timeline-todos__head { font-size: 12px; color: var(--muted); margin-bottom: 6px; }
.timeline-todos__list { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; }
.timeline-todo { display: flex; gap: 8px; font-size: 13px; }
.timeline-todo--completed .timeline-todo__text { color: var(--muted); text-decoration: line-through; }
.timeline-todo--in_progress .timeline-todo__mark { color: var(--accent); }
.timeline-notice { font-size: 12px; color: var(--muted); text-align: center; }
.timeline-notice--answered { color: var(--ok, #22c55e); }
.timeline-dots { display: flex; gap: 4px; padding: 6px 2px; }
.timeline-dots span { width: 5px; height: 5px; border-radius: 50%; background: var(--muted); animation: timeline-dot 1.2s infinite; }
.timeline-dots span:nth-child(2) { animation-delay: 0.15s; }
.timeline-dots span:nth-child(3) { animation-delay: 0.3s; }
@keyframes timeline-dot { 0%, 80%, 100% { opacity: 0.25; } 40% { opacity: 1; } }
@media (max-width: 860px) {
  .codex-layout { grid-template-columns: minmax(0, 1fr); }
}
```

`web/src/styles/chat.css` 追加：

```css
@import "./chat/codex.css";
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/timeline-render.test.ts
npm run type-check
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/ui/chat/timeline-render.ts web/src/ui/chat/timeline-render.test.ts \
  web/src/styles/chat/codex.css web/src/styles/chat.css
git commit -m "feat(web): render session timeline with tool and subagent blocks"
```

---

### Task 4: 提问卡片组件

**Files:**

- Create: `web/src/ui/chat/question-card.ts`
- Test: `web/src/ui/chat/question-card.test.ts`

**Interfaces:**

- Consumes: `Question`、`QuestionAnswer`、`answerQuestion`（Task 1）。
- Produces: `<question-card .sessionKey=${string} .pending=${PendingQuestion} @answered=${CustomEvent<void>}>`；内部调用 `answerQuestion`。

- [ ] **Step 1: 写失败测试**

`question-card.test.ts`：

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ answerQuestion: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ answerQuestion: hoisted.answerQuestion }));

import './question-card.js';
import type { QuestionCard } from './question-card.js';

async function mount(): Promise<QuestionCard> {
  const el = document.createElement('question-card') as QuestionCard;
  el.sessionKey = 'agent:ws-1:default/s1';
  el.pending = {
    questionId: 'q1',
    questions: [
      {
        question: '检查哪些设备？',
        header: '范围',
        options: [
          { label: '全部', description: '全部设备' },
          { label: '仅离线', description: '只检查离线' },
        ],
        multiSelect: false,
      },
    ],
  };
  document.body.appendChild(el);
  await el.updateComplete;
  return el;
}

describe('question-card', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    hoisted.answerQuestion.mockReset();
  });

  it('submits the selected option', async () => {
    hoisted.answerQuestion.mockResolvedValue(undefined);
    const el = await mount();
    (el.shadowRoot!.querySelectorAll('.question-option')[1] as HTMLButtonElement).click();
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.question-submit') as HTMLButtonElement).click();
    await el.updateComplete;
    expect(hoisted.answerQuestion).toHaveBeenCalledWith('agent:ws-1:default/s1', 'q1', {
      answers: [{ questionIndex: 0, selected: ['仅离线'] }],
    });
    expect(el.shadowRoot!.textContent).toContain('已提交');
  });

  it('disables submit until every question has a selection', async () => {
    const el = await mount();
    const submit = el.shadowRoot!.querySelector('.question-submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
  });
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/question-card.test.ts
```

Expected: FAIL，模块不存在。

- [ ] **Step 3: 写最小实现**

`web/src/ui/chat/question-card.ts`：

```ts
import { LitElement, css, html, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { answerQuestion, type QuestionAnswer } from '../../api/assistant.js';
import type { PendingQuestion } from './timeline-model.js';

@customElement('question-card')
export class QuestionCard extends LitElement {
  static styles = css`
    :host { display: block; width: 100%; max-width: 760px; margin: 0 auto 10px; }
    .card { border: 1px solid var(--accent); border-radius: var(--radius-md); background: var(--card); padding: 12px; }
    .head { font-size: 12px; color: var(--accent); margin-bottom: 8px; }
    .question { margin-bottom: 12px; }
    .question__title { font-size: 13.5px; margin-bottom: 6px; }
    .options { display: flex; flex-wrap: wrap; gap: 6px; }
    .question-option { border: 1px solid var(--border); background: var(--panel-strong); color: var(--text); border-radius: 999px; padding: 6px 12px; cursor: pointer; font-size: 12.5px; }
    .question-option[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
    .question-option__desc { display: block; font-size: 11px; color: var(--muted); }
    .footer { display: flex; justify-content: flex-end; }
    .question-submit { background: var(--accent); color: #fff; border: 0; border-radius: var(--radius-sm); padding: 6px 14px; cursor: pointer; }
    .question-submit:disabled { opacity: 0.5; cursor: default; }
    .error { color: var(--danger, #ef4444); font-size: 12px; }
  `;

  @property({ attribute: false }) sessionKey = '';
  @property({ attribute: false }) pending: PendingQuestion | null = null;

  @state() private selected: string[][] = [];
  @state() private submitting = false;
  @state() private submitted = false;
  @state() private error = '';

  protected willUpdate(changed: PropertyValues): void {
    if (changed.has('pending') && this.pending) {
      this.selected = this.pending.questions.map(() => []);
      this.submitted = false;
      this.error = '';
    }
  }

  private toggle(questionIndex: number, label: string, multiSelect: boolean): void {
    const next = this.selected.map((list) => [...list]);
    const current = next[questionIndex] ?? [];
    if (multiSelect) {
      next[questionIndex] = current.includes(label)
        ? current.filter((item) => item !== label)
        : [...current, label];
    } else {
      next[questionIndex] = [label];
    }
    this.selected = next;
  }

  private get complete(): boolean {
    if (!this.pending) return false;
    return this.selected.length === this.pending.questions.length && this.selected.every((list) => list.length > 0);
  }

  private async submit(): Promise<void> {
    if (!this.pending || !this.complete || this.submitting) return;
    this.submitting = true;
    this.error = '';
    const answer: QuestionAnswer = {
      answers: this.selected.map((selected, questionIndex) => ({ questionIndex, selected })),
    };
    try {
      await answerQuestion(this.sessionKey, this.pending.questionId, answer);
      this.submitted = true;
      this.dispatchEvent(new CustomEvent('answered'));
    } catch (error) {
      this.error = (error as Error).message || '提交回答失败';
    } finally {
      this.submitting = false;
    }
  }

  render() {
    if (!this.pending) return html``;
    return html`
      <section class="card" aria-label="需要你决定">
        <div class="head">需要你决定</div>
        ${this.pending.questions.map(
          (question, index) => html`
            <div class="question">
              <div class="question__title">${question.question}</div>
              <div class="options">
                ${question.options.map(
                  (option) => html`
                    <button
                      class="question-option"
                      aria-pressed=${String((this.selected[index] ?? []).includes(option.label))}
                      @click=${() => this.toggle(index, option.label, question.multiSelect)}
                    >
                      ${option.label}
                      ${option.description
                        ? html`<span class="question-option__desc">${option.description}</span>`
                        : ''}
                    </button>
                  `,
                )}
              </div>
            </div>
          `,
        )}
        ${this.error ? html`<div class="error">${this.error}</div>` : ''}
        <div class="footer">
          ${this.submitted
            ? html`<span class="question-option__desc">已提交，等待大脑继续</span>`
            : html`<button
                class="question-submit"
                ?disabled=${!this.complete || this.submitting}
                @click=${() => this.submit()}
              >
                ${this.submitting ? '提交中…' : '提交'}
              </button>`}
        </div>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'question-card': QuestionCard;
  }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/question-card.test.ts
npm run type-check
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/ui/chat/question-card.ts web/src/ui/chat/question-card.test.ts
git commit -m "feat(web): add inline question card"
```

---

### Task 5: 会话抽屉

**Files:**

- Create: `web/src/ui/chat/session-drawer.ts`
- Test: `web/src/ui/chat/session-drawer.test.ts`

**Interfaces:**

- Consumes: `SessionSummary`、`listChatSessions`、`renameChatSession`、`deleteChatSession`。
- Produces: `<session-drawer .agentId=${string} .current=${string} @session-select=${CustomEvent<string>} @session-deleted=${CustomEvent<string>}>`；暴露 `refresh(): Promise<void>`。

- [ ] **Step 1: 写失败测试**

`session-drawer.test.ts`：

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  listChatSessions: vi.fn(),
  renameChatSession: vi.fn(),
  deleteChatSession: vi.fn(),
}));
vi.mock('../../api/assistant.js', () => hoisted);

import './session-drawer.js';
import type { SessionDrawer } from './session-drawer.js';

describe('session-drawer', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    Object.values(hoisted).forEach((mock) => mock.mockReset());
  });

  it('groups sessions by kind and emits select', async () => {
    hoisted.listChatSessions.mockResolvedValue([
      { sessionKey: 'agent:ws:a/c1', kind: 'chat', status: 'active', label: '新会话', updatedAt: 2 },
      { sessionKey: 'agent:ws:a/s1', kind: 'scheduled', status: 'ended', label: '巡检', updatedAt: 1 },
    ]);
    const el = document.createElement('session-drawer') as SessionDrawer;
    el.agentId = 'default';
    el.current = 'agent:ws:a/c1';
    const selected: string[] = [];
    el.addEventListener('session-select', (event) => selected.push((event as CustomEvent<string>).detail));
    document.body.appendChild(el);
    await el.refresh();
    await el.updateComplete;

    expect(el.shadowRoot!.textContent).toContain('对话');
    expect(el.shadowRoot!.textContent).toContain('巡检');
    const items = el.shadowRoot!.querySelectorAll('.drawer-item');
    expect(items).toHaveLength(2);
    (items[1] as HTMLButtonElement).click();
    expect(selected).toEqual(['agent:ws:a/s1']);
  });
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/session-drawer.test.ts
```

Expected: FAIL，模块不存在。

- [ ] **Step 3: 写最小实现**

`web/src/ui/chat/session-drawer.ts`：

```ts
import { LitElement, css, html } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import {
  deleteChatSession,
  listChatSessions,
  renameChatSession,
  type SessionSummary,
} from '../../api/assistant.js';

const GROUPS: Array<{ kind: string; title: string }> = [
  { kind: 'chat', title: '对话' },
  { kind: 'scheduled', title: '巡检' },
  { kind: 'event', title: '事件' },
];

@customElement('session-drawer')
export class SessionDrawer extends LitElement {
  static styles = css`
    :host { display: flex; flex-direction: column; min-height: 0; border-right: 1px solid var(--border); padding-right: var(--space-3); }
    .head { display: flex; align-items: center; gap: 8px; padding: 4px 2px 10px; }
    .title { font-size: 12px; color: var(--muted); flex: 1; }
    .new { border: 1px solid var(--border); background: var(--card); color: var(--text); border-radius: var(--radius-sm); padding: 4px 10px; cursor: pointer; font-size: 12px; }
    .list { overflow-y: auto; min-height: 0; display: flex; flex-direction: column; gap: 10px; }
    .group__title { font-size: 11px; color: var(--muted); margin: 4px 0; text-transform: uppercase; letter-spacing: 0.04em; }
    .drawer-row { display: flex; align-items: flex-start; gap: 4px; border-radius: var(--radius-sm); }
    .drawer-row--current { background: var(--card); }
    .drawer-row:hover { background: var(--card); }
    .drawer-item { flex: 1; min-width: 0; text-align: left; background: transparent; border: 0; border-radius: var(--radius-sm); padding: 6px 8px; color: var(--text); cursor: pointer; font-size: 13px; }
    .drawer-row--current .drawer-item { color: var(--accent); }
    .drawer-item__meta { display: block; font-size: 11px; color: var(--muted); margin-top: 2px; }
    .drawer-actions { display: flex; flex-direction: column; gap: 2px; padding: 4px 4px 0 0; }
    .drawer-action { border: 0; background: transparent; color: var(--muted); font-size: 11px; cursor: pointer; padding: 0 2px; }
    .drawer-action:hover { color: var(--text); }
    .empty { font-size: 12px; color: var(--muted); padding: 8px; }
  `;

  @property() agentId = 'default';
  @property() current = '';

  @state() private sessions: SessionSummary[] = [];
  @state() private error = '';

  connectedCallback(): void {
    super.connectedCallback();
    void this.refresh();
  }

  async refresh(): Promise<void> {
    try {
      this.sessions = await listChatSessions(this.agentId);
      this.error = '';
    } catch (error) {
      this.error = (error as Error).message || '加载会话失败';
    }
  }

  private select(sessionKey: string): void {
    this.dispatchEvent(new CustomEvent<string>('session-select', { detail: sessionKey, bubbles: true, composed: true }));
  }

  private async rename(sessionKey: string, event: Event): Promise<void> {
    event.stopPropagation();
    const label = window.prompt('会话名称', this.sessions.find((item) => item.sessionKey === sessionKey)?.label ?? '');
    if (!label?.trim()) return;
    try {
      await renameChatSession(sessionKey, label.trim());
      await this.refresh();
    } catch (error) {
      this.error = (error as Error).message || '重命名失败';
    }
  }

  private async remove(sessionKey: string, event: Event): Promise<void> {
    event.stopPropagation();
    if (!window.confirm('删除这个会话？历史消息会一起删除。')) return;
    try {
      await deleteChatSession(sessionKey);
      this.dispatchEvent(new CustomEvent<string>('session-deleted', { detail: sessionKey, bubbles: true, composed: true }));
      await this.refresh();
    } catch (error) {
      this.error = (error as Error).message || '删除失败';
    }
  }

  render() {
    return html`
      <div class="head">
        <span class="title">会话</span>
        <button class="new" @click=${() => this.dispatchEvent(new CustomEvent('session-new', { bubbles: true, composed: true }))}>
          ＋ 新会话
        </button>
      </div>
      ${this.error ? html`<div class="empty">${this.error}</div>` : ''}
      <div class="list">
        ${GROUPS.map((group) => {
          const items = this.sessions.filter((session) => session.kind === group.kind);
          if (items.length === 0) return '';
          return html`
            <section class="group">
              <div class="group__title">${group.title}</div>
              ${items.map(
                (session) => html`
                  <div class="drawer-row ${session.sessionKey === this.current ? 'drawer-row--current' : ''}">
                    <button class="drawer-item" @click=${() => this.select(session.sessionKey)}>
                      ${session.label || session.sessionKey.split('/').pop() || session.sessionKey}
                      <span class="drawer-item__meta">
                        ${session.status === 'ended' ? '已结束' : '进行中'} · ${session.kind}
                      </span>
                    </button>
                    <span class="drawer-actions">
                      <button class="drawer-action" @click=${(event: Event) => this.rename(session.sessionKey, event)}>
                        改名
                      </button>
                      <button class="drawer-action" @click=${(event: Event) => this.remove(session.sessionKey, event)}>
                        删除
                      </button>
                    </span>
                  </div>
                `,
              )}
            </section>
          `;
        })}
        ${this.sessions.length === 0 ? html`<div class="empty">还没有会话</div>` : ''}
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'session-drawer': SessionDrawer;
  }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/chat/session-drawer.test.ts
npm run type-check
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/ui/chat/session-drawer.ts web/src/ui/chat/session-drawer.test.ts
git commit -m "feat(web): add session drawer with grouping and rename"
```

---

### Task 6: 全局 HUD 浮动卡

**Files:**

- Create: `web/src/ui/components/global-hud.ts`
- Test: `web/src/ui/components/global-hud.test.ts`

**Interfaces:**

- Consumes: `fetchAssistantHud`（Task 1）。
- Produces: `<global-hud .workspaceId=${string} pollMs=${number}>`；默认 10s 轮询；页面隐藏时暂停；点击展开详情（assistant 状态、调度、活动、待处理项、最近事件）。

- [ ] **Step 1: 写失败测试**

`global-hud.test.ts`：

```ts
import { afterEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({ fetchAssistantHud: vi.fn() }));
vi.mock('../../api/assistant.js', () => ({ fetchAssistantHud: hoisted.fetchAssistantHud }));

import './global-hud.js';
import type { GlobalHud } from './global-hud.js';

const snapshot = {
  assistant: { status: 'waiting_for_user', lastOutcome: 'completed' },
  devices: { total: 12, online: 10, offline: 2 },
  alarms: { highPriority: 3 },
  pending: { questions: 1, approvals: 0 },
  activity: { activeSessions: 1 },
  schedule: { enabled: true, intervalSeconds: 300, prompt: '', nextWakeAt: '2026-11-08T10:10:00Z', lastWakeAt: null, lastOutcome: null },
  recentEvents: [],
};

describe('global-hud', () => {
  afterEach(() => {
    document.body.innerHTML = '';
    hoisted.fetchAssistantHud.mockReset();
  });

  it('renders the global status and counters', async () => {
    hoisted.fetchAssistantHud.mockResolvedValue(snapshot);
    const el = document.createElement('global-hud') as GlobalHud;
    el.workspaceId = 'ws-1';
    el.pollMs = 0;
    document.body.appendChild(el);
    await el.refresh();
    await el.updateComplete;

    const text = el.shadowRoot!.textContent!;
    expect(text).toContain('等待你');
    expect(text).toContain('12');
    expect(text).toContain('3');
    expect(text).toContain('1 待你回答');
    el.remove();
  });

  it('expands the detail panel', async () => {
    hoisted.fetchAssistantHud.mockResolvedValue(snapshot);
    const el = document.createElement('global-hud') as GlobalHud;
    el.workspaceId = 'ws-1';
    el.pollMs = 0;
    document.body.appendChild(el);
    await el.refresh();
    await el.updateComplete;
    (el.shadowRoot!.querySelector('.hud-card') as HTMLButtonElement).click();
    await el.updateComplete;
    expect(el.shadowRoot!.textContent).toContain('下次唤醒');
    el.remove();
  });
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/components/global-hud.test.ts
```

Expected: FAIL，模块不存在。

- [ ] **Step 3: 写最小实现**

`web/src/ui/components/global-hud.ts`：

```ts
import { LitElement, css, html, nothing } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { fetchAssistantHud, type HudSnapshot } from '../../api/assistant.js';

const STATUS_LABEL: Record<string, string> = {
  idle: '空闲',
  working: '运行中',
  waiting_for_user: '等待你',
  paused: '已暂停',
  error: '异常',
};

function formatTime(value: string | null): string {
  if (!value) return '—';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

@customElement('global-hud')
export class GlobalHud extends LitElement {
  static styles = css`
    :host { position: absolute; top: 12px; left: 50%; transform: translateX(-50%); z-index: 20; width: min(560px, calc(100% - 32px)); }
    .hud-card { display: flex; flex-direction: column; gap: 4px; width: 100%; text-align: left; border: 1px solid var(--border); background: var(--glass-chat-input-bg, rgba(24, 27, 34, 0.72)); backdrop-filter: blur(18px); border-radius: var(--radius-lg); padding: 10px 12px; color: var(--text); cursor: pointer; box-shadow: var(--shadow-md, 0 8px 24px rgba(0,0,0,0.28)); }
    .hud-card:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
    .row { display: flex; align-items: center; gap: 10px; font-size: 12px; }
    .status { display: inline-flex; align-items: center; gap: 6px; font-weight: 600; }
    .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); }
    .dot--working { background: var(--accent); }
    .dot--waiting_for_user { background: var(--warn, #f59e0b); }
    .dot--error { background: var(--danger, #ef4444); }
    .dot--paused { background: var(--muted); }
    .muted { color: var(--muted); }
    .detail { margin-top: 6px; border-top: 1px solid var(--border); padding-top: 8px; display: grid; gap: 4px; font-size: 12px; }
    .detail__row { display: flex; justify-content: space-between; gap: 12px; }
    .detail__row span:first-child { color: var(--muted); }
    .events { margin: 4px 0 0; padding: 0; list-style: none; display: grid; gap: 2px; color: var(--muted); }
    .events li { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .error { color: var(--danger, #ef4444); }
  `;

  @property() workspaceId = '';
  @property({ type: Number }) pollMs = 10_000;

  @state() private snapshot: HudSnapshot | null = null;
  @state() private expanded = false;
  @state() private error = '';

  private timer: number | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    void this.refresh();
    if (this.pollMs > 0) {
      this.timer = window.setInterval(() => {
        if (document.visibilityState === 'visible') void this.refresh();
      }, this.pollMs);
    }
  }

  disconnectedCallback(): void {
    if (this.timer !== null) window.clearInterval(this.timer);
    super.disconnectedCallback();
  }

  async refresh(): Promise<void> {
    if (!this.workspaceId) return;
    try {
      this.snapshot = await fetchAssistantHud(this.workspaceId);
      this.error = '';
    } catch (error) {
      this.error = (error as Error).message || '加载全局状态失败';
    }
  }

  private toggle(event?: KeyboardEvent): void {
    if (event && event.key !== 'Enter' && event.key !== ' ') return;
    event?.preventDefault();
    this.expanded = !this.expanded;
  }

  render() {
    const snapshot = this.snapshot;
    if (!snapshot) {
      return html`<div
        class="hud-card"
        role="button"
        tabindex="0"
        @click=${() => this.toggle()}
        @keydown=${(event: KeyboardEvent) => this.toggle(event)}
      >
        <div class="row"><span class="muted">${this.error || '正在连接大脑…'}</span></div>
      </div>`;
    }
    const status = snapshot.assistant.status;
    return html`
      <div
        class="hud-card"
        role="button"
        tabindex="0"
        aria-expanded=${String(this.expanded)}
        @click=${() => this.toggle()}
        @keydown=${(event: KeyboardEvent) => this.toggle(event)}
      >
        <div class="row">
          <span class="status"><span class="dot dot--${status}"></span>${STATUS_LABEL[status] ?? status}</span>
          <span class="muted">下次 ${formatTime(snapshot.schedule.nextWakeAt)}</span>
          <span class="muted">设备 ${snapshot.devices.total} · 离线 ${snapshot.devices.offline}</span>
          <span class="muted">告警 ${snapshot.alarms.highPriority} 高</span>
          <span class="muted">${snapshot.pending.questions} 待你回答</span>
        </div>
        ${this.expanded
          ? html`<div class="detail">
              <div class="detail__row"><span>Assistant</span><span>${STATUS_LABEL[status] ?? status}</span></div>
              <div class="detail__row"><span>调度</span><span>${snapshot.schedule.enabled ? `每 ${Math.round(snapshot.schedule.intervalSeconds / 60)} 分钟` : '已关闭'}</span></div>
              <div class="detail__row"><span>下次唤醒</span><span>${formatTime(snapshot.schedule.nextWakeAt)}</span></div>
              <div class="detail__row"><span>活动会话</span><span>${snapshot.activity.activeSessions}</span></div>
              <div class="detail__row"><span>待处理</span><span>${snapshot.pending.questions} 问 · ${snapshot.pending.approvals} 批</span></div>
              <div class="detail__row"><span>最近唤醒</span><span>${snapshot.schedule.lastOutcome ?? '—'}</span></div>
              <ul class="events">
                ${snapshot.recentEvents.slice(0, 5).map((event) => html`<li>${event.eventType}</li>`)}
              </ul>
            </div>`
          : nothing}
        ${this.error ? html`<div class="error">${this.error}</div>` : nothing}
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'global-hud': GlobalHud;
  }
}
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/components/global-hud.test.ts
npm run type-check
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/ui/components/global-hud.ts web/src/ui/components/global-hud.test.ts
git commit -m "feat(web): add floating global hud"
```

---

### Task 7: 聊天视图重建（抽屉 + 时间线 + composer）

**Files:**

- Modify: `web/src/ui/views/chat.ts`
- Modify: `web/src/ui/app.ts`（聊天路由加 `content--chat`）
- Test: `web/src/ui/views/chat.test.ts`

**Interfaces:**

- Consumes: Task 1–6 的 API/模型/组件；现有 `createChatState`、`sendChatMessage`、`abortChatRun`、`loadChatHistory`、`resolveSessionKey`。
- Produces: 新的 `<view-chat>`：左侧 `<session-drawer>`，中间 `.codex-thread` 时间线，顶部 `<global-hud>`，底部 composer（发送/停止 + 只读「需要审批」徽标）。

- [ ] **Step 1: 写失败测试**

`web/src/ui/views/chat.test.ts`：

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const hoisted = vi.hoisted(() => ({
  fetchSessionEvents: vi.fn(),
  connectSessionEvents: vi.fn(() => () => {}),
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
        {
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
        },
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
});
```

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/views/chat.test.ts
```

Expected: FAIL，旧视图没有 timeline/抽屉/HUD。

- [ ] **Step 3: 写最小实现**

`web/src/ui/views/chat.ts` 改为（保留旧文件中的 welcome 文案可按需精简；Light DOM 便于全局样式）：

```ts
import { LitElement, html, type PropertyValues } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { getWorkspaceId } from '../../api/client.js';
import { connectSessionEvents, fetchSessionEvents, fetchSessionTodos } from '../../api/assistant.js';
import {
  abortChatRun,
  createChatState,
  loadChatHistory,
  sendChatMessage,
  type ChatState,
} from '../controllers/chat.js';
import { resolveSessionKey } from '../shared/session-key.js';
import { TimelineModel, timelineFromHistory, type PendingQuestion } from '../chat/timeline-model.js';
import { renderTimeline, renderTodoPanel } from '../chat/timeline-render.js';
import '../chat/session-drawer.js';
import '../chat/question-card.js';
import '../components/global-hud.js';

const AGENT_ID = 'default';

@customElement('view-chat')
export class ChatView extends LitElement {
  createRenderRoot() {
    return this;
  }

  @state() private sessionKey = '';
  @state() private draft = '';
  @state() private liveText = '';
  @state() private sending = false;
  @state() private error = '';

  private timeline = new TimelineModel();
  private chatState: ChatState | null = null;
  private closeEvents: (() => void) | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    void this.bootstrap();
  }

  disconnectedCallback(): void {
    this.closeEvents?.();
    this.closeEvents = null;
    super.disconnectedCallback();
  }

  private get workspaceId(): string {
    return getWorkspaceId() ?? '';
  }

  private async bootstrap(): Promise<void> {
    const workspaceId = this.workspaceId;
    const stored = localStorage.getItem('tinyiothub_chat_last_session');
    const sessionKey =
      stored && stored.startsWith(`agent:${workspaceId}:`) ? stored : resolveSessionKey(AGENT_ID);
    localStorage.setItem('tinyiothub_chat_last_session', sessionKey);
    this.selectSession(sessionKey);
  }

  private selectSession(sessionKey: string): void {
    this.closeEvents?.();
    this.timeline.reset();
    this.liveText = '';
    this.error = '';
    this.sessionKey = sessionKey;
    localStorage.setItem('tinyiothub_chat_last_session', sessionKey);
    this.chatState = createChatState(sessionKey, AGENT_ID);
    this.chatState.onChange = () => {
      this.liveText = this.chatState?.chatStream ?? '';
      this.sending = Boolean(this.chatState?.chatSending);
      this.error = this.chatState?.lastError ?? '';
      this.requestUpdate();
    };
    this.refreshDrawer();
    void this.loadTimeline(sessionKey);
    this.closeEvents = connectSessionEvents({
      sessionKey,
      after: 0,
      onEvent: (event) => {
        this.timeline.ingestEvent(event);
        void this.updateComplete;
      },
      onError: (error) => {
        this.error = error.message;
        void this.updateComplete;
      },
    });
  }

  private async loadTimeline(sessionKey: string): Promise<void> {
    try {
      let page = await fetchSessionEvents(sessionKey, 0, 200);
      this.timeline.ingest(page);
      let guard = 0;
      while (this.timeline.cursor < page.highWatermark && guard < 10) {
        page = await fetchSessionEvents(sessionKey, this.timeline.cursor, 200);
        if (page.events.length === 0) break;
        this.timeline.ingest(page);
        guard += 1;
      }
      const todos = await fetchSessionTodos(sessionKey);
      if (todos.length > 0 && this.timeline.todos.length === 0) this.timeline.todos = todos;
      if (this.timeline.blocks.length === 0 && this.chatState) {
        await loadChatHistory(this.chatState);
        this.timeline.blocks = timelineFromHistory(
          this.chatState.chatMessages as unknown as Array<{
            role: string;
            content?: Array<{ type: string; text?: string }>;
          }>,
        );
      }
      this.error = '';
    } catch (error) {
      this.error = (error as Error).message || '加载会话失败';
    }
    void this.updateComplete;
  }

  private async send(): Promise<void> {
    const message = this.draft.trim();
    if (!message || !this.chatState || this.sending) return;
    this.draft = '';
    this.error = '';
    await sendChatMessage(this.chatState, message);
    this.liveText = '';
    void this.loadTimeline(this.sessionKey);
  }

  private async stop(): Promise<void> {
    if (this.chatState) await abortChatRun(this.chatState);
  }

  private onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void this.send();
    }
  }

  private answerPending(): void {
    this.timeline.pendingQuestion = null;
    void this.loadTimeline(this.sessionKey);
  }

  private refreshDrawer(): void {
    const drawer = this.querySelector('session-drawer') as
      | (HTMLElement & { refresh(): Promise<void> })
      | null;
    void drawer?.refresh();
  }

  protected updated(_changed: PropertyValues): void {
    const thread = this.querySelector('.codex-thread');
    if (thread) thread.scrollTop = thread.scrollHeight;
  }

  render() {
    const pending: PendingQuestion | null = this.timeline.pendingQuestion;
    return html`
      <div class="codex-layout">
        <session-drawer
          .agentId=${AGENT_ID}
          .current=${this.sessionKey}
          @session-select=${(event: CustomEvent<string>) => this.selectSession(event.detail)}
          @session-new=${() => this.selectSession(resolveSessionKey(AGENT_ID))}
          @session-deleted=${(event: CustomEvent<string>) => {
            if (event.detail === this.sessionKey) this.selectSession(resolveSessionKey(AGENT_ID));
            else this.refreshDrawer();
          }}
        ></session-drawer>
        <main class="codex-main">
          <global-hud .workspaceId=${this.workspaceId}></global-hud>
          <div class="codex-thread">
            ${renderTodoPanel(this.timeline.todos)}
            ${pending
              ? html`<question-card
                  .sessionKey=${this.sessionKey}
                  .pending=${pending}
                  @answered=${() => this.answerPending()}
                ></question-card>`
              : ''}
            ${renderTimeline(this.timeline.blocks, { liveText: this.liveText, sending: this.sending })}
            ${this.error ? html`<div class="timeline-notice timeline-notice--timed_out">${this.error}</div>` : ''}
          </div>
          <div class="chat-input-area">
            <textarea
              class="chat-input"
              .value=${this.draft}
              placeholder="和大脑一起工作…"
              ?disabled=${this.sending}
              @input=${(event: Event) => (this.draft = (event.target as HTMLTextAreaElement).value)}
              @keydown=${(event: KeyboardEvent) => this.onKeydown(event)}
            ></textarea>
            <div class="codex-composer-meta">
              <span class="codex-mode" title="权限模式接线在 Plan 4">需要审批</span>
              ${this.sending
                ? html`<button class="chat-abort-btn" @click=${() => this.stop()}>停止</button>`
                : html`<button class="chat-send-btn" ?disabled=${!this.draft.trim()} @click=${() => this.send()}>
                    发送
                  </button>`}
            </div>
          </div>
        </main>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-chat': ChatView;
  }
}
```

> `loadChatHistory` 的返回值以现有签名为准（`Promise<void>` 或消息数组）；若它写入 `chatState.messages`，历史兜底改为读取 `chatState.messages` 并调用 `timelineFromHistory`。实现时以真实签名为准，保持「事件为空时才加载历史」的行为。

`web/src/ui/app.ts`：在 `renderPage()` 的 chat 分支外层内容容器加 `content--chat`（现有 `.content` 切换逻辑处），使聊天占满高度、无底部 padding。示例（以实际渲染函数为准）：

```ts
content.classList.toggle('content--chat', view === 'chat');
```

`codex.css` 追加 composer 相关样式：

```css
.codex-composer-meta { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
.codex-mode { font-size: 11px; color: var(--muted); border: 1px solid var(--border); border-radius: 999px; padding: 3px 8px; }
```

- [ ] **Step 4: 运行测试，确认通过**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx vitest run src/ui/views/chat.test.ts src/ui/controllers/chat.test.ts
npm run type-check
```

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add web/src/ui/views/chat.ts web/src/ui/views/chat.test.ts \
  web/src/ui/app.ts web/src/styles/chat/codex.css
git commit -m "feat(web): rebuild chat view around session timeline"
```

---

### Task 8: 端到端验证与构建

**Files:**

- Modify: `web/e2e/assistant.spec.ts`（新建）
- Test: `web/e2e/fixture-server.ts`（如需新增 fixture 路由，仅添加不删除）

**Interfaces:**

- Consumes: Task 1–7。
- Produces: Playwright smoke：聊天页渲染抽屉/时间线/HUD/composer，无控制台错误；构建通过。

- [ ] **Step 1: 写失败测试**

`web/e2e/assistant.spec.ts`：

```ts
import { expect, test } from '@playwright/test';

test('assistant workbench renders drawer, timeline, hud and composer', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => {
    localStorage.setItem('auth-token', 'test-token');
    localStorage.setItem('workspace-id', 'ws-e2e');
  });
  await page.goto('/#/chat');
  await expect(page.locator('view-chat session-drawer')).toBeVisible();
  await expect(page.locator('view-chat global-hud')).toBeVisible();
  await expect(page.locator('view-chat .chat-input')).toBeVisible();
  await expect(page.locator('view-chat .codex-thread')).toBeVisible();
});
```

> 若项目当前的 chat 路由不是 `#/chat`，按 `app.ts` 的实际路由写法调整（现有导航有「AI 助手」项，路由以 `handleRoute` 为准）。

- [ ] **Step 2: 运行测试，确认失败**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npx playwright test e2e/assistant.spec.ts
```

Expected: FAIL，选择器不存在或 fixture 未提供新端点（在 `e2e/fixture-server.ts` 中为 `/chat/sessions`、`/chat/sessions/*/events`、`/todos`、`/workspaces/*/agent/hud` 增加最小 JSON fixture；只加路由，不改现有行为）。

- [ ] **Step 3: 修正 fixture / 选择器**

在 `web/e2e/fixture-server.ts` 增加：

```ts
const assistantSessionKey = 'agent:ws-e2e:default/e2e';

// 在现有路由匹配中加入：
{
  const url = new URL(req.url ?? '', 'http://localhost:3002');
  if (url.pathname === '/api/v1/chat/sessions') {
    return json({ code: 0, msg: '', result: { sessions: [
      { session_key: assistantSessionKey, workspace_id: 'ws-e2e', agent_id: 'default', label: '巡检', kind: 'chat', status: 'active', created_at: 1, updated_at: 1 },
    ] } });
  }
  if (url.pathname === `/api/v1/chat/sessions/${encodeURIComponent(assistantSessionKey)}/events`) {
    return json({ events: [], highWatermark: 0 });
  }
  if (url.pathname === `/api/v1/chat/sessions/${encodeURIComponent(assistantSessionKey)}/todos`) {
    return json([]);
  }
  if (url.pathname === '/api/v1/workspaces/ws-e2e/agent/hud') {
    return json({
      assistant: { status: 'idle', lastOutcome: null },
      devices: { total: 3, online: 2, offline: 1 },
      alarms: { highPriority: 0 },
      pending: { questions: 0, approvals: 0 },
      activity: { activeSessions: 0 },
      schedule: { enabled: true, intervalSeconds: 300, prompt: '', nextWakeAt: null, lastWakeAt: null, lastOutcome: null },
      recentEvents: [],
    });
  }
}
```

（`json()` 为 fixture-server 现有辅助函数；若名称不同按现有工具改写。SSE 端点可返回一个保持打开的 `text/event-stream` 或直接 204；Plan 3 测试只要求页面渲染。）

- [ ] **Step 4: 运行全量 web 验证**

Run:

```bash
export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
cd /Users/chenguorong/code/my/tinyiothub/web
npm test
npm run type-check
npm run lint
npm run build
npx playwright test e2e/assistant.spec.ts
```

Expected: 全部通过；`vite build` 产物在 `dist/ui`。

- [ ] **Step 5: 提交**

```bash
git add web/e2e/assistant.spec.ts web/e2e/fixture-server.ts
git commit -m "test(web): add assistant workbench e2e smoke"
```

---

## 验证清单（执行者完成全部 Task 后逐项打勾）

- [ ] 聊天页是单列居中（约 760px），用户消息右侧胶囊、助手正文文档式。
- [ ] 左侧抽屉按「对话 / 巡检 / 事件」分组；可新建、选择、改名、删除会话。
- [ ] 时间线按 `session_events` 渲染 turn 分隔、工具一行摘要（可展开参数/结果）、skill 行、子代理卡片（可展开）、结束提示。
- [ ] Todo 内联在时间线上方，显示 `计划中 x/y`，状态变化随事件刷新。
- [ ] `question.asked` 显示内联卡片；选择后 POST 到答题端点；`question.answered`/超时后卡片消失并出现结果提示。
- [ ] 顶部 HUD 显示状态/设备/告警/待答/下次唤醒；点击展开详情；页面隐藏时暂停轮询。
- [ ] composer 发送/停止工作，SSE 文本增量仍实时显示；权限模式显示只读「需要审批」徽标（不提供假切换）。
- [ ] `npm test`、`npm run type-check`、`npm run lint`、`npm run build`、Playwright smoke 全部通过。
- [ ] 没有新增后端端点；没有删除旧组件文件；没有提交无关工作区文件。

---

## Self-Review

**Spec coverage（Plan 3 范围内）：**

- §10 版式规则 1–9：Task 3/5/7（单列、用户胶囊、todo 内联、工具一行、提问内联、子代理卡片、抽屉、深色 tokens、无假进度）。
- §10.2 全局 HUD：Task 6（状态/设备/告警/待办/活动/调度/最近事件/点击展开；仅在聊天主区域挂载）。
- §10.3 Composer：Task 7（`＋` 与模型选择不做；权限模式只读徽标，避免伪造）。
- §8 事件可见性：Task 2/3（timeline 由 `session_events` 驱动；文本增量仍用 `ChatEvent`）。
- §13 部分：`ask_user` 卡片让在线用户可回答；无人在线语义仍属 Plan 4。
- 验证：Task 8。
- **不在本计划**：权限模式强制、审批卡片、附件/模型切换、A2UI 重写、旧代码删除（Plan 4）。

**Placeholder scan：** 本计划没有 `TBD`/`TODO`/“实现细节略”。所有任务给出完整代码、命令与断言。

**Type consistency：**

- `SessionEvent`/`SessionEventPage`/`TodoItem`/`Question`/`QuestionAnswer`/`SessionSummary`/`HudSnapshot`/`ScheduleSnapshot` 在 Task 1 定义，Task 2/3/4/5/6/7 只引用。
- `TimelineBlock` 的 `kind` 在 Task 2 定义、Task 3 渲染；`toolSummary` 的入参类型为 `Extract<TimelineBlock, {kind:'tool'}>`。
- `PendingQuestion {questionId, questions}` 在 Task 2/4/7 一致。
- `question-card` 的事件名 `answered`、抽屉的 `session-select`/`session-new`/`session-deleted` 在 Task 4/5/7 一致。
- 原始端点的响应字段 camelCase（Task 1 类型）与后端 `HarnessEvent`/HUD DTO 一致。

**已知风险（接受并记录）：**

1. `session_events` 从 Plan 1 才开始写入；更早的会话没有事件，Task 7 用 `/chat/history` 兜底（仅文本）。
2. SSE 与 `ChatEvent` 双通道：文本以 `ChatEvent` 为准，过程以事件为准；短时间内工具行可能比文本晚一帧。
3. A2UI 仍依赖历史消息/`ChatEvent` 的 `canvas` 工具流，未迁入时间线；`canvas` 工具行在时间线中跳过。
4. 调度/巡检会话自动出现，但前端只在用户打开对应会话时建立 SSE；抽屉需要手动刷新或收到 `session.started` 时刷新（Task 7 在切换/删除时刷新）。
5. `npm run build` 需要 Node 24；CI/本地脚本必须显式切换。
