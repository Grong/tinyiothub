# Chat & Agent + A2UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Chat page (`/chat`) and Agent management page (`/agents`) to TinyIoTHub's Lit frontend, with SSE streaming, A2UI component rendering, and backend proxy to OpenClaw Gateway.

**Architecture:** TinyIoTHub frontend → TinyIoTHub backend (Rust/Axum) → OpenClaw Gateway. All chat/agent RPC calls go through backend proxy endpoints. Frontend uses Lit 3 Web Components (Light DOM), controller/view split, SSE for streaming.

**Tech Stack:** Lit 3, TypeScript, Rust/Axum, SSE, OpenClaw Gateway RPC

---

## File Structure

```
web/src/
├── ui/
│   ├── views/
│   │   ├── chat.ts                  # Chat view component
│   │   └── agents.ts                # Agent management view
│   ├── controllers/
│   │   ├── chat.ts                  # Chat state machine
│   │   └── agents.ts                # Agent CRUD controller
│   ├── chat/
│   │   ├── grouped-render.ts        # Message grouping rendering
│   │   ├── message-normalizer.ts    # Raw message normalization
│   │   └── a2ui/
│   │       ├── a2ui-renderer.ts     # A2UI JSON → Lit template dispatcher
│   │       └── catalog/
│   │           ├── text.ts
│   │           ├── button.ts
│   │           ├── card.ts
│   │           ├── column.ts
│   │           ├── row.ts
│   │           ├── divider.ts
│   │           ├── device-card.ts
│   │           ├── device-table.ts
│   │           ├── data-chart.ts
│   │           ├── control-panel.ts
│   │           ├── progress-indicator.ts
│   │           ├── confirmation-dialog.ts
│   │           └── index.ts
│   └── app.ts                       # Add /chat and /agents routes
├── styles/
│   ├── chat.css                     # Already exists
│   ├── chat/layout.css              # Already exists
│   ├── chat/text.css                # Already exists
│   ├── chat/grouped.css             # Already exists
│   ├── chat/tool-cards.css          # Already exists
│   └── chat/sidebar.css             # Already exists

api/src/api/
├── chat/
│   ├── mod.rs                       # Chat router
│   ├── proxy.rs                     # Chat proxy handlers
│   └── types.rs                     # Chat request/response types
└── mod.rs                           # Add chat route mount

api/src/infrastructure/
└── openclaw_agent.rs                # Extend trait with chat/agent proxy methods
```

---

## Phase 1: Backend Proxy Endpoints

### Task 1: Extend OpenClawAgentClient trait with chat proxy methods

**Files:**
- Modify: `api/src/infrastructure/openclaw_agent.rs`

Add these methods to the `OpenClawAgentClient` trait (following the existing `Pin<Box<dyn Future>>` pattern):

```rust
/// Send a chat message and get SSE stream
fn chat_send(
    &self,
    agent_id: &str,
    session_key: &str,
    message: &str,
    run_id: &str,
) -> Pin<Box<dyn std::future::Future<Output = Result<reqwest::Response, OpenClawError>> + Send + '_>>;

/// Get chat history
fn chat_history(
    &self,
    agent_id: &str,
    session_key: &str,
    limit: u32,
) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, OpenClawError>> + Send + '_>>;

/// Abort a chat run
fn chat_abort(
    &self,
    agent_id: &str,
    session_key: &str,
    run_id: Option<&str>,
) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>>;

/// List agents
fn list_agents(
    &self,
) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, OpenClawError>> + Send + '_>>;

/// Get agent config
fn get_agent_config(
    &self,
    agent_id: &str,
) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, OpenClawError>> + Send + '_>>;

/// Set agent config
fn set_agent_config(
    &self,
    agent_id: &str,
    config: &str,
    base_hash: Option<&str>,
) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>>;

/// Get tools catalog
fn tools_catalog(
    &self,
    agent_id: &str,
) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, OpenClawError>> + Send + '_>>;

/// Get effective tools
fn tools_effective(
    &self,
    agent_id: &str,
) -> Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, OpenClawError>> + Send + '_>>;

/// Toggle a tool
fn tools_toggle(
    &self,
    agent_id: &str,
    tool_name: &str,
    enabled: bool,
) -> Pin<Box<dyn std::future::Future<Output = Result<(), OpenClawError>> + Send + '_>>;
```

Implement each method on `RealOpenClawAgentClient` using `do_request` or direct `self.client.request()` for SSE streaming. Implement each method on `MockOpenClawAgentClient` as stubs returning `Err(OpenClawError::Unavailable("mock".into()))`.

Also add `chat_send` that returns `reqwest::Response` (not deserialized) for SSE streaming — use `self.client.post(url).send().await` directly, returning the raw response.

- [ ] **Step 1: Add trait methods to OpenClawAgentClient**

Add all 9 methods to the trait in `openclaw_agent.rs`. Each returns `Pin<Box<dyn Future<Output = Result<...>> + Send + '_>>`.

- [ ] **Step 2: Implement on RealOpenClawAgentClient**

For JSON endpoints (`chat_history`, `list_agents`, `get_agent_config`, `tools_catalog`, `tools_effective`): use `do_request` with `GET` or `POST`.

For `chat_send`: construct URL `{base_url}/api/v1/chat/stream`, POST with JSON body `{agent_id, session_key, message, run_id}`, return raw `reqwest::Response`.

For mutations (`chat_abort`, `set_agent_config`, `tools_toggle`): use `do_request` with `POST`/`PUT`.

- [ ] **Step 3: Implement stubs on MockOpenClawAgentClient**

Each method returns `Err(OpenClawError::Unavailable("mock".into()))`. For `chat_send`, return `Err(...)` as well.

- [ ] **Step 4: Verify compilation**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo build`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add api/src/infrastructure/openclaw_agent.rs
git commit -m "feat(chat): extend OpenClawAgentClient with chat/agent proxy methods"
```

---

### Task 2: Create chat proxy endpoint types and router

**Files:**
- Create: `api/src/api/chat/types.rs`
- Create: `api/src/api/chat/proxy.rs`
- Create: `api/src/api/chat/mod.rs`
- Modify: `api/src/api/mod.rs`

- [ ] **Step 1: Create `api/src/api/chat/types.rs`**

```rust
use serde::{Deserialize, Serialize};

/// Request body for POST /api/v1/chat/stream
#[derive(Debug, Deserialize)]
pub struct ChatStreamRequest {
    pub agent_id: String,
    pub session_key: String,
    pub message: String,
    pub run_id: String,
}

/// Request body for GET /api/v1/chat/history
#[derive(Debug, Deserialize)]
pub struct ChatHistoryQuery {
    pub agent_id: String,
    pub session_key: String,
    pub limit: Option<u32>,
}

/// Request body for POST /api/v1/chat/abort
#[derive(Debug, Deserialize)]
pub struct ChatAbortRequest {
    pub agent_id: String,
    pub session_key: String,
    pub run_id: Option<String>,
}

/// Request body for PUT /api/v1/agents/:id/config
#[derive(Debug, Deserialize)]
pub struct AgentConfigUpdateRequest {
    pub config: serde_json::Value,
    pub base_hash: Option<String>,
}

/// Request body for POST /api/v1/tools/toggle
#[derive(Debug, Deserialize)]
pub struct ToolToggleRequest {
    pub agent_id: String,
    pub tool_name: String,
    pub enabled: bool,
}
```

- [ ] **Step 2: Create `api/src/api/chat/proxy.rs`**

```rust
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response, Sse},
    Json,
};
use async_stream::stream;
use futures::StreamExt;

use crate::{
    api::middleware::workspace::WorkspaceScope,
    dto::response::{api_response::ApiResponse, builder::ApiResponseBuilder},
    shared::{app_state::AppState, security::jwt::Claims},
};

use super::types::*;

/// POST /api/v1/chat/stream — SSE streaming chat
pub async fn chat_stream(
    State(state): State<AppState>,
    claims: Claims,
    Json(req): Json<ChatStreamRequest>,
) -> Response {
    let client = state.get_openclaw_agent_client();
    let response = match client.chat_send(&req.agent_id, &req.session_key, &req.message, &req.run_id).await {
        Ok(resp) => resp,
        Err(e) => {
            return ApiResponseBuilder::<()>::error(&format!("Chat stream failed: {}", e));
        }
    };

    // Forward the SSE stream from OpenClaw to the client
    let byte_stream = response.bytes_stream();
    let event_stream = stream! {
        let mut stream = byte_stream;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                        for line in text.lines() {
                            yield Ok::<_, std::io::Error>(axum::sse::Event::default().data(line));
                        }
                    }
                }
                Err(e) => {
                    yield Ok(axum::sse::Event::default().data(format!("error: {}", e)));
                    break;
                }
            }
        }
    };

    Sse::new(event_stream).into_response()
}

/// GET /api/v1/chat/history
pub async fn chat_history(
    State(state): State<AppState>,
    Query(query): Query<ChatHistoryQuery>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let client = state.get_openclaw_agent_client();
    let limit = query.limit.unwrap_or(200);
    match client.chat_history(&query.agent_id, &query.session_key, limit).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to load chat history: {}", e)),
    }
}

/// POST /api/v1/chat/abort
pub async fn chat_abort(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ChatAbortRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let client = state.get_openclaw_agent_client();
    let run_id_ref = req.run_id.as_deref();
    match client.chat_abort(&req.agent_id, &req.session_key, run_id_ref).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"aborted": true})),
        Err(e) => ApiResponseBuilder::error(&format!("Abort failed: {}", e)),
    }
}

/// GET /api/v1/agents
pub async fn list_agents(
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let client = state.get_openclaw_agent_client();
    match client.list_agents().await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to list agents: {}", e)),
    }
}

/// GET /api/v1/agents/:id/config
pub async fn get_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let client = state.get_openclaw_agent_client();
    match client.get_agent_config(&agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to get agent config: {}", e)),
    }
}

/// PUT /api/v1/agents/:id/config
pub async fn set_agent_config(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    _claims: Claims,
    Json(req): Json<AgentConfigUpdateRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let client = state.get_openclaw_agent_client();
    let config_str = serde_json::to_string(&req.config).unwrap_or_default();
    let base_hash_ref = req.base_hash.as_deref();
    match client.set_agent_config(&agent_id, &config_str, base_hash_ref).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"saved": true})),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to save config: {}", e)),
    }
}

/// GET /api/v1/tools/catalog
pub async fn tools_catalog(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    let client = state.get_openclaw_agent_client();
    match client.tools_catalog(agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to get tools catalog: {}", e)),
    }
}

/// GET /api/v1/tools/effective
pub async fn tools_effective(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    _claims: Claims,
) -> Json<ApiResponse<serde_json::Value>> {
    let agent_id = params.get("agent_id").map(|s| s.as_str()).unwrap_or("");
    let client = state.get_openclaw_agent_client();
    match client.tools_effective(agent_id).await {
        Ok(data) => ApiResponseBuilder::success(data),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to get effective tools: {}", e)),
    }
}

/// POST /api/v1/tools/toggle
pub async fn tools_toggle(
    State(state): State<AppState>,
    _claims: Claims,
    Json(req): Json<ToolToggleRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let client = state.get_openclaw_agent_client();
    match client.tools_toggle(&req.agent_id, &req.tool_name, req.enabled).await {
        Ok(()) => ApiResponseBuilder::success(serde_json::json!({"toggled": true})),
        Err(e) => ApiResponseBuilder::error(&format!("Failed to toggle tool: {}", e)),
    }
}
```

- [ ] **Step 3: Create `api/src/api/chat/mod.rs`**

```rust
pub mod proxy;
pub mod types;

use axum::{routing::{get, post, put}, Router};
use crate::shared::app_state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/stream", post(proxy::chat_stream))
        .route("/history", get(proxy::chat_history))
        .route("/abort", post(proxy::chat_abort))
}
```

- [ ] **Step 4: Mount routes in `api/src/api/mod.rs`**

Add `pub mod chat;` to the module declarations. Add these to `protected_routes`:

```rust
.nest("/chat", chat::create_router())
.route("/agents", get(chat::proxy::list_agents))
.route("/agents/:id/config", get(chat::proxy::get_agent_config).put(chat::proxy::set_agent_config))
.route("/tools/catalog", get(chat::proxy::tools_catalog))
.route("/tools/effective", get(chat::proxy::tools_effective))
.route("/tools/toggle", post(chat::proxy::tools_toggle))
```

- [ ] **Step 5: Verify compilation**

Run: `cd /Users/chenguorong/code/my/tinyiothub/api && cargo build`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add api/src/api/chat/ api/src/api/mod.rs
git commit -m "feat(chat): add backend proxy endpoints for chat and agents"
```

---

## Phase 2: Chat Page — Controller, View, Message Rendering

### Task 3: Create chat controller

**Files:**
- Create: `web/src/ui/controllers/chat.ts`

The chat controller manages chat state as a plain object (matching OpenClaw's pattern). It calls the TinyIoTHub backend proxy (not OpenClaw directly). Uses `apiPost`, `apiGet` from `../../api/client.js`.

- [ ] **Step 1: Create `web/src/ui/controllers/chat.ts`**

```typescript
import { apiGet, apiPost } from "../../api/client.js";
import type { ChatAttachment } from "../ui-types.js";

export type ChatMessage = {
  role: string;
  content: Array<{ type: string; text?: string; [key: string]: unknown }>;
  timestamp?: number;
  toolCallId?: string;
  toolName?: string;
  senderLabel?: string;
};

export type ChatEventPayload = {
  runId: string;
  sessionKey: string;
  state: "delta" | "final" | "aborted" | "error";
  message?: ChatMessage;
  errorMessage?: string;
  a2ui?: string;
};

export type ChatState = {
  sessionKey: string;
  agentId: string;
  chatLoading: boolean;
  chatMessages: ChatMessage[];
  chatSending: boolean;
  chatRunId: string | null;
  chatStream: string | null;
  chatStreamStartedAt: number | null;
  lastError: string | null;
};

export function createChatState(sessionKey: string, agentId: string): ChatState {
  return {
    sessionKey,
    agentId,
    chatLoading: false,
    chatMessages: [],
    chatSending: false,
    chatRunId: null,
    chatStream: null,
    chatStreamStartedAt: null,
    lastError: null,
  };
}

export async function loadChatHistory(state: ChatState): Promise<void> {
  state.chatLoading = true;
  state.lastError = null;
  try {
    const res = await apiGet<{ messages?: ChatMessage[] }>("/chat/history", {
      agent_id: state.agentId,
      session_key: state.sessionKey,
      limit: 200,
    });
    const messages = Array.isArray(res.result?.messages) ? res.result.messages : [];
    state.chatMessages = messages.filter((m) => !isSilentReply(m));
    state.chatStream = null;
    state.chatStreamStartedAt = null;
  } catch (err) {
    state.lastError = String(err);
  } finally {
    state.chatLoading = false;
  }
}

export function sendChatMessage(
  state: ChatState,
  message: string,
  attachments?: ChatAttachment[],
): { runId: string; stream: EventSource | ReadableStream } | null {
  const msg = message.trim();
  if (!msg && (!attachments || attachments.length === 0)) return null;

  const now = Date.now();
  const runId = crypto.randomUUID();

  // Optimistic: add user message immediately
  const contentBlocks: ChatMessage["content"] = [];
  if (msg) contentBlocks.push({ type: "text", text: msg });

  state.chatMessages = [
    ...state.chatMessages,
    { role: "user", content: contentBlocks, timestamp: now },
  ];

  state.chatSending = true;
  state.lastError = null;
  state.chatRunId = runId;
  state.chatStream = "";
  state.chatStreamStartedAt = now;

  // POST to /chat/stream, read SSE response
  const token = sessionStorage.getItem("token") || localStorage.getItem("token") || "";
  const baseUrl = (import.meta as any).env?.VITE_API_BASE || "/api/v1";

  const controller = new AbortController();

  fetch(`${baseUrl}/chat/stream`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      agent_id: state.agentId,
      session_key: state.sessionKey,
      message: msg,
      run_id: runId,
    }),
    signal: controller.signal,
  })
    .then(async (response) => {
      if (!response.body) throw new Error("No response body");
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          if (line.startsWith("data: ")) {
            const data = line.slice(6);
            try {
              const payload: ChatEventPayload = JSON.parse(data);
              handleChatEvent(state, payload);
            } catch {
              // skip non-JSON lines
            }
          }
        }
      }
    })
    .catch((err) => {
      if (err.name !== "AbortError") {
        state.lastError = String(err);
        state.chatRunId = null;
        state.chatStream = null;
      }
    })
    .finally(() => {
      state.chatSending = false;
    });

  return { runId, stream: new ReadableStream() }; // caller can abort via controller
}

export function handleChatEvent(state: ChatState, payload: ChatEventPayload): void {
  if (payload.sessionKey !== state.sessionKey) return;

  // Cross-run final event
  if (payload.runId && state.chatRunId && payload.runId !== state.chatRunId) {
    if (payload.state === "final" && payload.message && !isSilentReply(payload.message)) {
      state.chatMessages = [...state.chatMessages, payload.message];
    }
    return;
  }

  if (payload.state === "delta") {
    const text = extractText(payload.message);
    if (text && !isSilentReplyText(text)) {
      state.chatStream = text;
    }
  } else if (payload.state === "final") {
    if (payload.message && !isSilentReply(payload.message)) {
      state.chatMessages = [...state.chatMessages, payload.message];
    } else if (state.chatStream?.trim() && !isSilentReplyText(state.chatStream)) {
      state.chatMessages = [
        ...state.chatMessages,
        {
          role: "assistant",
          content: [{ type: "text", text: state.chatStream }],
          timestamp: Date.now(),
        },
      ];
    }
    state.chatStream = null;
    state.chatRunId = null;
    state.chatStreamStartedAt = null;
  } else if (payload.state === "aborted") {
    if (payload.message && !isSilentReply(payload.message)) {
      state.chatMessages = [...state.chatMessages, payload.message];
    } else if (state.chatStream?.trim()) {
      state.chatMessages = [
        ...state.chatMessages,
        {
          role: "assistant",
          content: [{ type: "text", text: state.chatStream }],
          timestamp: Date.now(),
        },
      ];
    }
    state.chatStream = null;
    state.chatRunId = null;
    state.chatStreamStartedAt = null;
  } else if (payload.state === "error") {
    state.lastError = payload.errorMessage || "Unknown error";
    state.chatStream = null;
    state.chatRunId = null;
    state.chatStreamStartedAt = null;
  }
}

export async function abortChatRun(state: ChatState): Promise<boolean> {
  try {
    await apiPost("/chat/abort", {
      agent_id: state.agentId,
      session_key: state.sessionKey,
      run_id: state.chatRunId,
    });
    return true;
  } catch (err) {
    state.lastError = String(err);
    return false;
  }
}

const SILENT_REPLY_PATTERN = /^\s*NO_REPLY\s*$/;

function isSilentReplyText(text: string): boolean {
  return SILENT_REPLY_PATTERN.test(text);
}

function isSilentReply(message: ChatMessage | undefined | null): boolean {
  if (!message) return false;
  const role = (message.role || "").toLowerCase();
  if (role !== "assistant") return false;
  const text = extractText(message);
  return typeof text === "string" && isSilentReplyText(text);
}

function extractText(message: ChatMessage | undefined | null): string {
  if (!message) return "";
  if (Array.isArray(message.content)) {
    return message.content
      .filter((c) => c.type === "text" && typeof c.text === "string")
      .map((c) => c.text!)
      .join("");
  }
  return "";
}
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No TypeScript errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/controllers/chat.ts
git commit -m "feat(chat): add chat controller with SSE streaming"
```

---

### Task 4: Create message normalizer

**Files:**
- Create: `web/src/ui/chat/message-normalizer.ts`

Normalizes raw chat messages into a consistent structure for rendering.

- [ ] **Step 1: Create `web/src/ui/chat/message-normalizer.ts`**

```typescript
export type NormalizedMessage = {
  role: string;
  content: NormalizedContentItem[];
  timestamp: number;
  id?: string;
  senderLabel?: string | null;
};

export type NormalizedContentItem = {
  type: string;
  text?: string;
  name?: string;
  args?: string;
};

export function normalizeMessage(message: unknown): NormalizedMessage {
  const m = message as Record<string, unknown>;
  let role = typeof m.role === "string" ? m.role : "unknown";

  // Detect tool messages
  const hasToolId = typeof m.toolCallId === "string" || typeof m.tool_call_id === "string";
  const hasToolName = typeof m.toolName === "string" || typeof m.tool_name === "string";
  const contentRaw = m.content;
  const contentItems = Array.isArray(contentRaw) ? contentRaw : null;
  const hasToolContent =
    Array.isArray(contentItems) &&
    contentItems.some((item) => {
      const x = item as Record<string, unknown>;
      const t = (x.type as string) || "";
      return t.startsWith("tool_") || t === "tool_result" || t === "tool_use";
    });

  if (hasToolId || hasToolContent || hasToolName) {
    role = "toolResult";
  }

  let content: NormalizedContentItem[] = [];

  if (typeof m.content === "string") {
    content = [{ type: "text", text: m.content }];
  } else if (Array.isArray(m.content)) {
    content = m.content.map((item: Record<string, unknown>) => ({
      type: (item.type as string) || "text",
      text: item.text as string | undefined,
      name: item.name as string | undefined,
      args: typeof item.args === "string" ? item.args : item.args ? JSON.stringify(item.args) : undefined,
    }));
  } else if (typeof m.text === "string") {
    content = [{ type: "text", text: m.text }];
  }

  const timestamp = typeof m.timestamp === "number" ? m.timestamp : Date.now();
  const id = typeof m.id === "string" ? m.id : undefined;
  const senderLabel =
    typeof m.senderLabel === "string" && m.senderLabel.trim() ? m.senderLabel.trim() : null;

  return { role, content, timestamp, id, senderLabel };
}

export function normalizeRoleForGrouping(role: string): string {
  const lower = role.toLowerCase();
  if (lower === "assistant" || lower === "model") return "assistant";
  if (lower === "user") return "user";
  if (lower === "toolresult" || lower === "tool_result") return "tool";
  return lower;
}

export function isToolResultMessage(message: NormalizedMessage): boolean {
  return message.role === "toolResult" || message.role === "tool";
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/ui/chat/message-normalizer.ts
git commit -m "feat(chat): add message normalizer"
```

---

### Task 5: Create grouped message renderer

**Files:**
- Create: `web/src/ui/chat/grouped-render.ts`

Renders grouped chat messages (Slack-style consecutive same-role grouping). Uses `lit` for templates and `marked` + `highlight.js` for markdown rendering.

- [ ] **Step 1: Install DOMPurify if needed**

Check if DOMPurify is in package.json. If not: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm add dompurify && pnpm add -D @types/dompurify`

- [ ] **Step 2: Create `web/src/ui/chat/grouped-render.ts`**

```typescript
import { html, nothing, type TemplateResult } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";
import { marked } from "marked";
import DOMPurify from "dompurify";
import type { NormalizedMessage } from "./message-normalizer.js";
import { normalizeMessage, normalizeRoleForGrouping } from "./message-normalizer.js";

export type MessageGroup = {
  role: string;
  messages: NormalizedMessage[];
  firstTimestamp: number;
};

// Configure marked with highlight.js
marked.setOptions({
  async: false,
  gfm: true,
});

function toMarkdownHtml(text: string): string {
  const raw = marked.parse(text) as string;
  return DOMPurify.sanitize(raw);
}

export function groupMessages(messages: unknown[]): MessageGroup[] {
  const groups: MessageGroup[] = [];
  let currentGroup: MessageGroup | null = null;

  for (const raw of messages) {
    const msg = normalizeMessage(raw);
    const normalizedRole = normalizeRoleForGrouping(msg.role);

    if (currentGroup && currentGroup.role === normalizedRole) {
      currentGroup.messages.push(msg);
    } else {
      currentGroup = {
        role: normalizedRole,
        messages: [msg],
        firstTimestamp: msg.timestamp,
      };
      groups.push(currentGroup);
    }
  }

  return groups;
}

export function renderMessageGroup(group: MessageGroup): TemplateResult {
  const isUser = group.role === "user";
  const isAssistant = group.role === "assistant";
  const isTool = group.role === "tool";

  const avatarIcon = isUser ? "U" : isAssistant ? "A" : "T";
  const avatarClass = isUser ? "chat-avatar--user" : isAssistant ? "chat-avatar--assistant" : "chat-avatar--tool";

  return html`
    <div class="chat-group ${group.role}">
      <div class="chat-avatar ${avatarClass}">${avatarIcon}</div>
      <div class="chat-group-messages">
        ${group.messages.map((msg) => renderSingleMessage(msg, isTool))}
      </div>
    </div>
  `;
}

function renderSingleMessage(msg: NormalizedMessage, isTool: boolean): TemplateResult {
  if (isTool) {
    return renderToolMessage(msg);
  }

  return html`
    <div class="chat-bubble ${msg.role === 'user' ? 'chat-bubble--user' : 'chat-bubble--assistant'}">
      ${msg.content.map((item) => {
        if (item.type === "text" && item.text) {
          return html`<div class="chat-content">${unsafeHTML(toMarkdownHtml(item.text))}</div>`;
        }
        return nothing;
      })}
      ${msg.timestamp ? html`
        <div class="chat-timestamp">${formatTime(msg.timestamp)}</div>
      ` : nothing}
    </div>
  `;
}

function renderToolMessage(msg: NormalizedMessage): TemplateResult {
  const toolName = msg.content.find((c) => c.name)?.name || "Tool";
  const args = msg.content.find((c) => c.args)?.args;
  const text = msg.content.find((c) => c.type === "text" && c.text)?.text;

  return html`
    <div class="chat-tool-card">
      <div class="chat-tool-card__header">
        <span class="chat-tool-card__icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
            <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>
          </svg>
        </span>
        <span class="chat-tool-card__name">${toolName}</span>
      </div>
      ${args ? html`<pre class="chat-tool-card__args">${args}</pre>` : nothing}
      ${text ? html`<div class="chat-tool-card__result">${unsafeHTML(toMarkdownHtml(text))}</div>` : nothing}
    </div>
  `;
}

export function renderStreamingGroup(text: string, startedAt: number): TemplateResult {
  return html`
    <div class="chat-group assistant">
      <div class="chat-avatar chat-avatar--assistant">A</div>
      <div class="chat-group-messages">
        <div class="chat-bubble chat-bubble--assistant">
          <div class="chat-content">${unsafeHTML(toMarkdownHtml(text))}</div>
          <div class="chat-streaming-indicator" aria-hidden="true">
            <span></span><span></span><span></span>
          </div>
        </div>
      </div>
    </div>
  `;
}

export function renderReadingIndicatorGroup(): TemplateResult {
  return html`
    <div class="chat-group assistant">
      <div class="chat-avatar chat-avatar--assistant">A</div>
      <div class="chat-group-messages">
        <div class="chat-bubble chat-reading-indicator" aria-hidden="true">
          <span class="chat-reading-indicator__dots">
            <span></span><span></span><span></span>
          </span>
        </div>
      </div>
    </div>
  `;
}

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}
```

- [ ] **Step 3: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/chat/grouped-render.ts
git commit -m "feat(chat): add grouped message renderer with markdown"
```

---

### Task 6: Create chat view component

**Files:**
- Create: `web/src/ui/views/chat.ts`

The chat view is a LitElement with Light DOM (`createRenderRoot() { return this; }`). It manages local UI state (draft, scroll position) and delegates to the chat controller for API calls. Includes sidebar for session list.

- [ ] **Step 1: Create `web/src/ui/views/chat.ts`**

Key implementation details:
- `@customElement("view-chat")`
- `createRenderRoot() { return this; }` (Light DOM)
- `@state() chatState: ChatState` — controller state
- `@state() draft: string` — input draft
- `@state() sessionsList: Array<{key: string; label: string}>` — session sidebar
- `@state() sessionKey: string` — current session
- `@state() sidebarCollapsed: boolean`
- `@state() a2uiSurfaces: Map<string, TemplateResult>` — A2UI surfaces (placeholder for Phase 3)

Methods:
- `firstUpdated()` — load sessions list, initialize chat state
- `handleSend()` — call `sendChatMessage`, update state reactively
- `handleAbort()` — call `abortChatRun`
- `handleNewSession()` — generate new sessionKey
- `scrollToBottom()` — auto-scroll on new messages
- `render()` — main layout: sidebar + chat area + input bar

The render method should use `groupMessages` and `renderMessageGroup` from `grouped-render.ts`. The input bar uses a `<textarea>` with Enter-to-send, Shift+Enter for newline.

Template structure:
```html
<div class="chat-layout">
  <div class="chat-sidebar">
    <!-- session list -->
    <button class="chat-new-session-btn">新建会话</button>
    ${this.sessionsList.map(s => html`
      <div class="chat-session-item ${s.key === this.sessionKey ? 'active' : ''}"
           @click=${() => this.switchSession(s.key)}>
        ${s.label}
      </div>
    `)}
  </div>
  <div class="chat-main">
    <div class="chat-messages" id="chatMessages">
      ${this.chatState.chatLoading ? html`<div class="chat-loading">加载中...</div>` : nothing}
      ${groups.map(g => renderMessageGroup(g))}
      ${this.chatState.chatStream ? renderStreamingGroup(this.chatState.chatStream, this.chatState.chatStreamStartedAt || Date.now()) : nothing}
    </div>
    <div class="chat-input-area">
      <textarea class="chat-input"
        .value=${this.draft}
        @input=${(e: Event) => { this.draft = (e.target as HTMLTextAreaElement).value; }}
        @keydown=${(e: KeyboardEvent) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); this.handleSend(); } }}
        placeholder="输入消息..."
      ></textarea>
      ${this.chatState.chatSending
        ? html`<button class="chat-abort-btn" @click=${this.handleAbort}>停止</button>`
        : html`<button class="chat-send-btn" @click=${this.handleSend} ?disabled=${!this.draft.trim()}>发送</button>`
      }
    </div>
  </div>
</div>
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/chat.ts
git commit -m "feat(chat): add chat view component with SSE streaming"
```

---

## Phase 3: A2UI Rendering Layer

### Task 7: Create A2UI catalog components (basic)

**Files:**
- Create: `web/src/ui/chat/a2ui/catalog/text.ts`
- Create: `web/src/ui/chat/a2ui/catalog/button.ts`
- Create: `web/src/ui/chat/a2ui/catalog/card.ts`
- Create: `web/src/ui/chat/a2ui/catalog/column.ts`
- Create: `web/src/ui/chat/a2ui/catalog/row.ts`
- Create: `web/src/ui/chat/a2ui/catalog/divider.ts`
- Create: `web/src/ui/chat/a2ui/catalog/index.ts`

Each catalog component exports a render function: `(data: Record<string, unknown>) => TemplateResult`.

- [ ] **Step 1: Create basic catalog components**

**`text.ts`:**
```typescript
import { html, type TemplateResult } from "lit";

export function renderA2uiText(data: Record<string, unknown>): TemplateResult {
  const text = String(data.text || "");
  const style = data.style as string | undefined;
  if (style === "heading") return html`<h3 class="a2ui-heading">${text}</h3>`;
  if (style === "subtitle") return html`<p class="a2ui-subtitle">${text}</p>`;
  if (style === "caption") return html`<small class="a2ui-caption">${text}</small>`;
  return html`<p class="a2ui-text">${text}</p>`;
}
```

**`button.ts`:**
```typescript
import { html, type TemplateResult } from "lit";

export type A2uiButtonData = {
  text?: string;
  variant?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  onClick?: () => void;
};

export function renderA2uiButton(data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void): TemplateResult {
  const text = String(data.text || "Button");
  const variant = String(data.variant || "primary");
  const disabled = Boolean(data.disabled);
  const functionId = data.functionId as string | undefined;

  return html`
    <button class="a2ui-btn a2ui-btn--${variant}"
      ?disabled=${disabled}
      @click=${() => { if (functionId && onAction) onAction(functionId, {}); }}>
      ${text}
    </button>
  `;
}
```

**`card.ts`, `column.ts`, `row.ts`, `divider.ts`:** Similar pattern — simple layout wrappers using `html` tagged templates.

**`index.ts`** — Registry:
```typescript
import type { TemplateResult } from "lit";
import { renderA2uiText } from "./text.js";
import { renderA2uiButton } from "./button.js";
import { renderA2uiCard } from "./card.js";
import { renderA2uiColumn } from "./column.js";
import { renderA2uiRow } from "./row.js";
import { renderA2uiDivider } from "./divider.js";

export type A2uiRenderer = (data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void) => TemplateResult;

export const a2uiCatalog: Record<string, A2uiRenderer> = {
  Text: renderA2uiText,
  Button: renderA2uiButton,
  Card: renderA2uiCard,
  Column: renderA2uiColumn,
  Row: renderA2uiRow,
  Divider: renderA2uiDivider,
};
```

- [ ] **Step 2: Create IoT extended components**

**`device-card.ts`:**
```typescript
import { html, type TemplateResult } from "lit";
import { unsafeHTML } from "lit/directives/unsafe-html.js";

export function renderDeviceCard(data: Record<string, unknown>): TemplateResult {
  const deviceId = String(data.deviceId || "");
  const deviceName = String(data.deviceName || deviceId);
  const status = String(data.status || "unknown");
  const isOnline = status === "online";

  return html`
    <div class="a2ui-device-card">
      <div class="a2ui-device-card__header">
        <span class="a2ui-device-card__status ${isOnline ? 'online' : 'offline'}"></span>
        <span class="a2ui-device-card__name">${deviceName}</span>
      </div>
      <div class="a2ui-device-card__id">${deviceId}</div>
    </div>
  `;
}
```

**`device-table.ts`, `data-chart.ts`, `control-panel.ts`, `progress-indicator.ts`, `confirmation-dialog.ts`:** Each renders a compact UI component. Register them all in `index.ts`.

- [ ] **Step 3: Register IoT components in catalog index**

Add to `a2uiCatalog` in `index.ts`:
```typescript
import { renderDeviceCard } from "./device-card.js";
import { renderDeviceTable } from "./device-table.js";
import { renderDataChart } from "./data-chart.js";
import { renderControlPanel } from "./control-panel.js";
import { renderProgressIndicator } from "./progress-indicator.js";
import { renderConfirmationDialog } from "./confirmation-dialog.js";

// In a2uiCatalog:
DeviceCard: renderDeviceCard,
DeviceTable: renderDeviceTable,
DataChart: renderDataChart,
ControlPanel: renderControlPanel,
ProgressIndicator: renderProgressIndicator,
ConfirmationDialog: renderConfirmationDialog,
```

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/chat/a2ui/
git commit -m "feat(a2ui): add A2UI catalog components (basic + IoT)"
```

---

### Task 8: Create A2UI renderer

**Files:**
- Create: `web/src/ui/chat/a2ui/a2ui-renderer.ts`

The A2UI renderer parses JSONL messages from the SSE `a2ui` field and manages surfaces/components.

- [ ] **Step 1: Create `web/src/ui/chat/a2ui/a2ui-renderer.ts`**

```typescript
import { html, nothing, type TemplateResult } from "lit";
import { a2uiCatalog, type A2uiRenderer } from "./catalog/index.js";

export type A2uiSurface = {
  id: string;
  surfaceKind: "inline" | "overlay";
  components: A2uiComponent[];
};

export type A2uiComponent = {
  id: string;
  componentKind: string;
  dataModel: Record<string, unknown>;
};

export class A2uiRendererEngine {
  private surfaces: Map<string, A2uiSurface> = new Map();
  private onAction?: (functionId: string, args: Record<string, unknown>) => void;

  constructor(onAction?: (functionId: string, args: Record<string, unknown>) => void) {
    this.onAction = onAction;
  }

  handleA2uiMessage(jsonl: string): void {
    const lines = jsonl.split("\n").filter((l) => l.trim());
    for (const line of lines) {
      try {
        const msg = JSON.parse(line);
        this.handleSingleMessage(msg);
      } catch {
        // skip non-JSON lines
      }
    }
  }

  private handleSingleMessage(msg: Record<string, unknown>): void {
    if (msg.createSurface) {
      const s = msg.createSurface as Record<string, unknown>;
      this.surfaces.set(s.id as string, {
        id: s.id as string,
        surfaceKind: (s.surfaceKind as string) || "inline",
        components: [],
      });
    } else if (msg.updateComponents) {
      const u = msg.updateComponents as Record<string, unknown>;
      const components = u.components as Array<Record<string, unknown>>;
      for (const comp of components) {
        // Find which surface this component belongs to
        for (const surface of this.surfaces.values()) {
          const idx = surface.components.findIndex((c) => c.id === comp.id);
          const a2uiComp: A2uiComponent = {
            id: comp.id as string,
            componentKind: comp.componentKind as string,
            dataModel: (comp.dataModel as Record<string, unknown>) || {},
          };
          if (idx >= 0) {
            surface.components[idx] = a2uiComp;
          } else {
            surface.components.push(a2uiComp);
          }
        }
      }
    } else if (msg.updateDataModel) {
      const u = msg.updateDataModel as Record<string, unknown>;
      const componentId = u.componentId as string;
      const dataModel = u.dataModel as Record<string, unknown>;
      for (const surface of this.surfaces.values()) {
        const comp = surface.components.find((c) => c.id === componentId);
        if (comp) {
          comp.dataModel = { ...comp.dataModel, ...dataModel };
        }
      }
    } else if (msg.deleteSurface) {
      const d = msg.deleteSurface as Record<string, unknown>;
      this.surfaces.delete(d.id as string);
    }
  }

  renderSurface(surfaceId: string): TemplateResult | typeof nothing {
    const surface = this.surfaces.get(surfaceId);
    if (!surface) return nothing;

    return html`
      <div class="a2ui-surface a2ui-surface--${surface.surfaceKind}">
        ${surface.components.map((comp) => this.renderComponent(comp))}
      </div>
    `;
  }

  renderAllSurfaces(): TemplateResult[] {
    const results: TemplateResult[] = [];
    for (const [id] of this.surfaces) {
      const rendered = this.renderSurface(id);
      if (rendered !== nothing) {
        results.push(rendered as TemplateResult);
      }
    }
    return results;
  }

  private renderComponent(comp: A2uiComponent): TemplateResult {
    const renderer: A2uiRenderer | undefined = a2uiCatalog[comp.componentKind];
    if (!renderer) {
      return html`<div class="a2ui-unknown">Unknown component: ${comp.componentKind}</div>`;
    }
    return renderer(comp.dataModel, this.onAction);
  }

  clear(): void {
    this.surfaces.clear();
  }

  hasSurfaces(): boolean {
    return this.surfaces.size > 0;
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add web/src/ui/chat/a2ui/a2ui-renderer.ts
git commit -m "feat(a2ui): add A2UI renderer engine"
```

---

### Task 9: Integrate A2UI into chat view

**Files:**
- Modify: `web/src/ui/views/chat.ts`

Add A2UI rendering to the chat view. When SSE events contain an `a2ui` field, pass it to the `A2uiRendererEngine`. Render A2UI surfaces inline within chat messages.

- [ ] **Step 1: Update chat view to use A2UI renderer**

In `chat.ts` view:
- Import `A2uiRendererEngine`
- Add `@state() a2uiRenderer = new A2uiRendererEngine()`
- In the chat controller's `handleChatEvent`, when `payload.a2ui` exists, call `this.a2uiRenderer.handleA2uiMessage(payload.a2ui)`
- In the render method, after message groups, render `this.a2uiRenderer.renderAllSurfaces()`

- [ ] **Step 2: Update chat controller to extract a2ui from SSE events**

In `controllers/chat.ts`, update `ChatEventPayload` to include `a2ui?: string`. In `handleChatEvent`, the a2ui field is already typed — no code change needed in the controller, just the view picks it up.

- [ ] **Step 3: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add web/src/ui/views/chat.ts
git commit -m "feat(chat): integrate A2UI rendering into chat view"
```

---

## Phase 4: Agents Page

### Task 10: Create agents controller

**Files:**
- Create: `web/src/ui/controllers/agents.ts`

- [ ] **Step 1: Create `web/src/ui/controllers/agents.ts`**

```typescript
import { apiGet, apiPost, apiPut } from "../../api/client.js";
import type { AgentsListResult, ToolsCatalogResult, ToolCatalogGroup } from "../types.js";

export type AgentsPanel = "overview" | "files" | "tools" | "skills" | "channels" | "cron";

export type AgentConfig = {
  model?: string;
  alternativeModels?: string[];
  workspace?: string;
  skills?: string[];
  tools?: {
    profile?: string;
    allow?: string[];
    alsoAllow?: string[];
    deny?: string[];
  };
  [key: string]: unknown;
};

export type AgentsState = {
  agentsLoading: boolean;
  agentsError: string | null;
  agentsList: AgentsListResult | null;
  selectedAgentId: string | null;
  activePanel: AgentsPanel;
  config: AgentConfig | null;
  configLoading: boolean;
  configDirty: boolean;
  configBaseHash: string | null;
  toolsCatalog: ToolCatalogGroup[] | null;
  toolsCatalogLoading: boolean;
};

export function createAgentsState(): AgentsState {
  return {
    agentsLoading: false,
    agentsError: null,
    agentsList: null,
    selectedAgentId: null,
    activePanel: "overview",
    config: null,
    configLoading: false,
    configDirty: false,
    configBaseHash: null,
    toolsCatalog: null,
    toolsCatalogLoading: false,
  };
}

export async function loadAgents(state: AgentsState): Promise<void> {
  state.agentsLoading = true;
  state.agentsError = null;
  try {
    const res = await apiGet<AgentsListResult>("/agents");
    state.agentsList = res.result || null;
    if (state.agentsList?.agents?.length && !state.selectedAgentId) {
      state.selectedAgentId = state.agentsList.agents[0].id;
    }
  } catch (err) {
    state.agentsError = String(err);
  } finally {
    state.agentsLoading = false;
  }
}

export async function loadAgentConfig(state: AgentsState, agentId: string): Promise<void> {
  state.configLoading = true;
  try {
    const res = await apiGet<{ config: AgentConfig; baseHash?: string }>(`/agents/${agentId}/config`);
    state.config = res.result?.config || null;
    state.configBaseHash = res.result?.baseHash || null;
    state.configDirty = false;
  } catch (err) {
    state.agentsError = String(err);
  } finally {
    state.configLoading = false;
  }
}

export async function saveAgentConfig(state: AgentsState, agentId: string): Promise<boolean> {
  if (!state.config) return false;
  try {
    await apiPut(`/agents/${agentId}/config`, {
      config: state.config,
      base_hash: state.configBaseHash,
    });
    state.configDirty = false;
    return true;
  } catch (err) {
    state.agentsError = String(err);
    return false;
  }
}

export async function loadToolsCatalog(state: AgentsState, agentId: string): Promise<void> {
  state.toolsCatalogLoading = true;
  try {
    const res = await apiGet<{ groups: ToolCatalogGroup[] }>("/tools/catalog", { agent_id: agentId });
    state.toolsCatalog = res.result?.groups || null;
  } catch (err) {
    state.agentsError = String(err);
  } finally {
    state.toolsCatalogLoading = false;
  }
}

export async function toggleTool(agentId: string, toolName: string, enabled: boolean): Promise<void> {
  await apiPost("/tools/toggle", { agent_id: agentId, tool_name: toolName, enabled });
}
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/controllers/agents.ts
git commit -m "feat(agents): add agents controller"
```

---

### Task 11: Create agents view

**Files:**
- Create: `web/src/ui/views/agents.ts`

- [ ] **Step 1: Create `web/src/ui/views/agents.ts`**

Key implementation details:
- `@customElement("view-agents")`
- `createRenderRoot() { return this; }`
- `@state() state: AgentsState`
- `@state() searchFilter: string`

`firstUpdated()` calls `loadAgents(this.state)`.

Renders:
1. **Agent selector** — horizontal list of agent pills at top
2. **6-panel tab bar** — horizontal pills using `.agent-tabs`/`.agent-tab` CSS
3. **Panel content** based on `activePanel`:
   - `overview`: model selector chips, workspace display, save/reload buttons
   - `files`: placeholder (file tree + editor — future phase)
   - `tools`: tools catalog grouped by section, toggle switches
   - `skills`: filterable skill list with toggles
   - `channels`: channel status cards
   - `cron`: cron job list with run-now buttons

Template structure:
```html
<div class="agents-layout">
  <div class="agents-header">
    <h2>Agent 管理</h2>
    <div class="agents-selector">
      ${(this.state.agentsList?.agents || []).map(a => html`
        <button class="agent-pill ${a.id === this.state.selectedAgentId ? 'active' : ''}"
                @click=${() => this.selectAgent(a.id)}>
          ${a.name || a.id}
        </button>
      `)}
    </div>
  </div>

  <div class="agent-tabs">
    ${(["overview", "files", "tools", "skills", "channels", "cron"] as AgentsPanel[]).map(panel => html`
      <button class="agent-tab ${this.state.activePanel === panel ? 'active' : ''}"
              @click=${() => { this.state = { ...this.state, activePanel: panel}; }}>
        ${panelLabels[panel]}
      </button>
    `)}
  </div>

  <div class="agent-panel-content">
    ${this.state.activePanel === "overview" ? this.renderOverview() : nothing}
    ${this.state.activePanel === "tools" ? this.renderTools() : nothing}
    ${this.state.activePanel === "files" ? this.renderFiles() : nothing}
    ${this.state.activePanel === "skills" ? this.renderSkills() : nothing}
    ${this.state.activePanel === "channels" ? this.renderChannels() : nothing}
    ${this.state.activePanel === "cron" ? this.renderCron() : nothing}
  </div>
</div>
```

Panel labels: `{ overview: "概览", files: "文件", tools: "工具", skills: "技能", channels: "渠道", cron: "定时任务" }`

**`renderOverview()`:** Shows model chips, config save button with dirty indicator.

**`renderTools()`:** Shows tools grouped by catalog sections, each with a toggle switch.

**`renderFiles()`, `renderSkills()`, `renderChannels()`, `renderCron()`:** Placeholder stubs with "即将推出" for now, structured for future expansion.

- [ ] **Step 2: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/ui/views/agents.ts
git commit -m "feat(agents): add agents view with 6-panel layout"
```

---

## Phase 5: Route Integration

### Task 12: Add routes and sidebar entries

**Files:**
- Modify: `web/src/ui/app.ts`

- [ ] **Step 1: Add view imports to app.ts**

Add side-effect imports after existing view imports:
```typescript
import "./views/chat.js";
import "./views/agents.js";
```

- [ ] **Step 2: Add "AI 助手" nav group to NAV_GROUPS**

Add a new group between "监控告警" and "系统管理":
```typescript
{
  label: "AI 助手",
  items: [
    { route: "chat", label: "AI 聊天", icon: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" },
    { route: "agents", label: "Agent 管理", icon: "M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z" },
  ],
},
```

- [ ] **Step 3: Add route handlers to renderPage()**

Add before the default fallback in `renderPage()`:
```typescript
if (route === "chat") return html`<view-chat></view-chat>`;
if (route === "agents") return html`<view-agents></view-agents>`;
```

- [ ] **Step 4: Verify build**

Run: `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add web/src/ui/app.ts
git commit -m "feat: add /chat and /agents routes with sidebar navigation"
```

---

### Task 13: Add A2UI CSS styles

**Files:**
- Modify: `web/src/styles/components.css` (or create `web/src/styles/chat/a2ui.css`)

- [ ] **Step 1: Add A2UI component styles**

```css
/* A2UI surfaces */
.a2ui-surface {
  margin: 8px 0;
  padding: 12px;
  border-radius: 8px;
  background: var(--bg-subtle);
}

.a2ui-surface--overlay {
  position: fixed;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  z-index: 1000;
  background: var(--bg-card);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  max-width: 600px;
  width: 90%;
}

/* A2UI basic components */
.a2ui-text { margin: 4px 0; }
.a2ui-heading { margin: 8px 0 4px; font-size: 1.1em; font-weight: 600; }
.a2ui-subtitle { margin: 4px 0; color: var(--muted); }
.a2ui-caption { color: var(--muted); font-size: 0.85em; }

.a2ui-btn {
  padding: 6px 16px;
  border-radius: 6px;
  border: none;
  cursor: pointer;
  font-size: 0.9em;
  font-weight: 500;
  transition: opacity 0.15s;
}
.a2ui-btn:hover { opacity: 0.85; }
.a2ui-btn--primary { background: var(--accent); color: white; }
.a2ui-btn--secondary { background: var(--bg-subtle); color: var(--text); border: 1px solid var(--border); }
.a2ui-btn--danger { background: #e74c3c; color: white; }

.a2ui-divider { border: none; border-top: 1px solid var(--border); margin: 8px 0; }

/* A2UI IoT components */
.a2ui-device-card {
  padding: 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-card);
}
.a2ui-device-card__header { display: flex; align-items: center; gap: 8px; }
.a2ui-device-card__status {
  width: 8px; height: 8px; border-radius: 50%;
}
.a2ui-device-card__status.online { background: #2ecc71; }
.a2ui-device-card__status.offline { background: #95a5a6; }
.a2ui-device-card__name { font-weight: 600; }
.a2ui-device-card__id { font-size: 0.8em; color: var(--muted); margin-top: 4px; }

.a2ui-unknown {
  padding: 8px;
  border: 1px dashed var(--border);
  color: var(--muted);
  font-size: 0.85em;
  border-radius: 4px;
}
```

- [ ] **Step 2: If creating new file, import it in `web/index.html` or wherever CSS is loaded**

- [ ] **Step 3: Commit**

```bash
git add web/src/styles/
git commit -m "feat(chat): add A2UI component styles"
```

---

## Final Verification

- [ ] Run `cd /Users/chenguorong/code/my/tinyiothub/web && pnpm build` — no TypeScript errors
- [ ] Run `cd /Users/chenguorong/code/my/tinyiothub/api && cargo build` — no Rust errors
- [ ] Browser test: navigate to `/chat` — page loads, input bar visible
- [ ] Browser test: navigate to `/agents` — page loads, agent list visible
- [ ] Sidebar shows "AI 助手" group with "AI 聊天" and "Agent 管理" entries
