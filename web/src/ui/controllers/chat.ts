import { apiGet, apiPost, buildUrl, getAuthToken } from "../../api/client.js";
import type { ChatAttachment } from "../ui-types.js";

// ============================================================================
// Types
// ============================================================================

export type ChatMessage = {
  role: string;
  content: Array<{ type: string; text?: string; name?: string; args?: unknown; result?: string; [key: string]: unknown }>;
  timestamp?: number;
  toolCallId?: string;
  toolName?: string;
  senderLabel?: string;
};

export type ToolStreamEntry = {
  toolCallId: string;
  toolName: string;
  toolArgs: string;
  toolResult?: string;
  startedAt: number;
};

export type ChatEventPayload = {
  runId: string;
  sessionKey: string;
  state: "delta" | "final" | "aborted" | "error" | "tool_call_start" | "tool_call_delta" | "tool_call_end" | "tool_result";
  message?: ChatMessage;
  errorMessage?: string;
  a2ui?: string;
  toolName?: string;
  toolArgs?: string;
  toolResults?: Array<{ toolName: string; toolArgs: string; result: string }>;
  result?: string;
};

export type ChatState = {
  sessionKey: string;
  agentId: string;
  chatLoading: boolean;
  chatMessages: ChatMessage[];
  chatSending: boolean;
  chatRunId: string | null;
  // 流式文本（当前正在接收的）
  chatStream: string | null;
  chatStreamStartedAt: number | null;
  // 已 committed 的流式文本片段（工具调用之前的文本）
  chatStreamSegments: Array<{ text: string; ts: number }>;
  // 工具调用流
  toolStreamById: Map<string, ToolStreamEntry>;
  toolStreamOrder: string[];
  lastError: string | null;
  onA2ui?: (jsonl: string) => void;
  lastA2uiSurfaceId?: string;
  a2uiChunks: string[];  // accumulated A2UI JSONL for current response
  abortController?: AbortController;
};

// ============================================================================
// State
// ============================================================================

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
    chatStreamSegments: [],
    toolStreamById: new Map(),
    toolStreamOrder: [],
    lastError: null,
    a2uiChunks: [],
  };
}

// ============================================================================
// History
// ============================================================================

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
    // Filter out system messages and silent replies
    state.chatMessages = messages.filter((m) => {
      const role = (m.role || "").toLowerCase();
      if (role === "system") return false; // Exclude system messages
      return !isSilentReply(m);
    });
    state.chatStream = null;
    state.chatStreamStartedAt = null;
    state.chatStreamSegments = [];
    state.toolStreamById = new Map();
    state.toolStreamOrder = [];
  } catch (err) {
    state.lastError = String(err);
  } finally {
    state.chatLoading = false;
  }
}

// ============================================================================
// Send
// ============================================================================

export function sendChatMessage(
  state: ChatState,
  message: string,
  attachments?: ChatAttachment[],
): { runId: string } | null {
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

  // Reset streaming state
  state.chatSending = true;
  state.lastError = null;
  state.chatRunId = runId;
  state.chatStream = "";
  state.chatStreamStartedAt = now;
  state.chatStreamSegments = [];
  state.toolStreamById = new Map();
  state.toolStreamOrder = [];
  state.a2uiChunks = [];

  const controller = new AbortController();
  state.abortController = controller;

  fetch(buildUrl("/chat/stream"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${getAuthToken() || ""}`,
    },
    body: JSON.stringify({
      agent_id: state.agentId,
      session_key: state.sessionKey,
      message: msg,
      run_id: runId,
      system_prompt: null,
    }),
    signal: controller.signal,
  })
    .then(async (response) => {
      if (response.status === 401) {
        sessionStorage.removeItem("auth-token");
        localStorage.removeItem("auth-token");
        window.dispatchEvent(new CustomEvent("auth-error", { detail: { message: "认证已过期" } }));
        throw new Error("Unauthorized - 请重新登录");
      }
      if (!response.ok) {
        let errMsg = `HTTP ${response.status}`;
        try {
          const errData = await response.json();
          errMsg = errData?.msg || errData?.message || errMsg;
        } catch { /* not JSON */ }
        throw new Error(errMsg);
      }
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
            } catch { /* skip non-JSON */ }
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

  return { runId };
}

// ============================================================================
// Event Handler
// ============================================================================

export function handleChatEvent(state: ChatState, payload: ChatEventPayload): void {
  if (payload.sessionKey !== state.sessionKey) return;

  // Cross-run: final from a different run → append as message
  if (payload.runId && state.chatRunId && payload.runId !== state.chatRunId) {
    if (payload.state === "final" && payload.message && !isSilentReply(payload.message)) {
      state.chatMessages = [...state.chatMessages, payload.message];
    }
    return;
  }

  switch (payload.state) {
    case "delta": {
      const text = extractText(payload.message);
      if (text && !isSilentReplyText(text)) {
        state.chatStream = (state.chatStream || "") + text;
      }
      break;
    }

    case "final": {
      flushStreamingToSegments(state);
      const toolMsgs = buildToolMessages(state);
      // Capture A2UI state before resetting
      const a2uiSurfaceId = state.lastA2uiSurfaceId;
      const a2uiJsonl = state.a2uiChunks.length > 0
        ? state.a2uiChunks.join("\n")
        : undefined;

      // Build extra fields for the message
      const a2uiFields: Record<string, unknown> = {};
      if (a2uiSurfaceId) a2uiFields.a2uiSurfaceId = a2uiSurfaceId;
      if (a2uiJsonl) a2uiFields.a2ui = a2uiJsonl;

      if (payload.message && !isSilentReply(payload.message)) {
        const msgWithA2ui = Object.keys(a2uiFields).length > 0
          ? { ...payload.message, ...a2uiFields } as ChatMessage
          : payload.message;
        state.chatMessages = [...state.chatMessages, ...toolMsgs, msgWithA2ui];
      } else if (state.chatStreamSegments.length > 0 || state.chatStream?.trim()) {
        const allText = [
          ...state.chatStreamSegments.map((s) => s.text),
          state.chatStream || "",
        ].join("");
        if (!isSilentReplyText(allText)) {
          const assistantMsg: ChatMessage = {
            role: "assistant",
            content: [{ type: "text", text: allText }],
            timestamp: Date.now(),
            ...a2uiFields,
          };
          state.chatMessages = [
            ...state.chatMessages,
            ...toolMsgs,
            assistantMsg,
          ];
        } else {
          state.chatMessages = [...state.chatMessages, ...toolMsgs];
        }
      } else if (toolMsgs.length > 0) {
        state.chatMessages = [...state.chatMessages, ...toolMsgs];
      }
      // Clear a2ui state after attaching to message
      state.lastA2uiSurfaceId = undefined;
      state.a2uiChunks = [];
      state.chatStream = null;
      state.chatStreamSegments = [];
      state.toolStreamById = new Map();
      state.toolStreamOrder = [];
      state.chatRunId = null;
      state.chatStreamStartedAt = null;
      break;
    }

    case "aborted": {
      flushStreamingToSegments(state);
      if (payload.message && !isSilentReply(payload.message)) {
        const toolMsgs = buildToolMessages(state);
        state.chatMessages = [...state.chatMessages, ...toolMsgs, payload.message];
      } else if (state.chatStream?.trim()) {
        const allText = [...state.chatStreamSegments.map((s) => s.text), state.chatStream].join("");
        if (!isSilentReplyText(allText)) {
          const toolMsgs = buildToolMessages(state);
          state.chatMessages = [
            ...state.chatMessages,
            ...toolMsgs,
            { role: "assistant", content: [{ type: "text", text: allText }], timestamp: Date.now() },
          ];
        }
      }
      state.chatStream = null;
      state.chatStreamSegments = [];
      state.toolStreamById = new Map();
      state.toolStreamOrder = [];
      state.chatRunId = null;
      state.chatStreamStartedAt = null;
      state.lastA2uiSurfaceId = undefined;
      break;
    }

    case "error": {
      state.lastError = payload.errorMessage || "Unknown error";
      state.chatStream = null;
      state.chatStreamSegments = [];
      state.toolStreamById = new Map();
      state.toolStreamOrder = [];
      state.chatRunId = null;
      state.chatStreamStartedAt = null;
      state.lastA2uiSurfaceId = undefined;
      break;
    }

    case "tool_call_start": {
      // Pause streaming text: commit current stream as a segment
      flushStreamingToSegments(state);
      const toolName = payload.toolName || "unknown";
      const toolArgs = payload.toolArgs || "";
      // Deduplicate: skip if we have already seen this (name, args) pair
      const seen = new Set(state.toolStreamOrder.map((id) => {
        const tc = state.toolStreamById.get(id);
        return tc ? `${tc.toolName}::${tc.toolArgs}` : "";
      }));
      const key = `${toolName}::${toolArgs}`;
      if (seen.has(key)) {
        // Duplicate — skip adding to stream state, but still process A2UI
        if (payload.a2ui && state.onA2ui) {
          const surfaceId = extractA2uiSurfaceId(payload.a2ui);
          if (surfaceId) state.lastA2uiSurfaceId = surfaceId;
          state.a2uiChunks.push(payload.a2ui);
          state.onA2ui(payload.a2ui);
        }
        break;
      }
      const toolCallId = `tc_${Date.now()}_${state.toolStreamOrder.length}`;
      state.toolStreamById.set(toolCallId, {
        toolCallId,
        toolName,
        toolArgs,
        startedAt: Date.now(),
      });
      state.toolStreamOrder.push(toolCallId);
      state.chatStream = null;

      // Handle A2UI canvas events from canvas tool
      if (payload.a2ui && state.onA2ui) {
        const surfaceId = extractA2uiSurfaceId(payload.a2ui);
        if (surfaceId) state.lastA2uiSurfaceId = surfaceId;
        console.log("[A2UI] Received a2ui in tool_call_start, surfaceId:", surfaceId, "payload.a2ui:", payload.a2ui);
        state.a2uiChunks.push(payload.a2ui);
        state.onA2ui(payload.a2ui);
      }
      break;
    }

    case "tool_call_end":
    case "tool_result": {
      // Attach tool results — match results to calls by sequential order
      if (payload.toolResults) {
        for (const result of payload.toolResults) {
          for (const id of state.toolStreamOrder) {
            const tc = state.toolStreamById.get(id);
            if (tc && !tc.toolResult) {
              tc.toolResult = result.result;
              break;
            }
          }
        }
      } else if (payload.state === "tool_result" && payload.toolName) {
        // Backend sends tool_result with toolName and result directly — match by sequential order
        for (const id of state.toolStreamOrder) {
          const tc = state.toolStreamById.get(id);
          if (tc && !tc.toolResult && tc.toolName === payload.toolName) {
            tc.toolResult = (payload as unknown as { result?: string }).result || "完成";
            break;
          }
        }
      }
      break;
    }
  }
}

// ============================================================================
// Abort
// ============================================================================

export async function abortChatRun(state: ChatState): Promise<boolean> {
  if (state.abortController) {
    state.abortController.abort();
    state.abortController = undefined;
  }
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

// ============================================================================
// Helpers
// ============================================================================

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

/// Commit any in-progress streaming text as a segment
function flushStreamingToSegments(state: ChatState): void {
  if (state.chatStream && state.chatStream.trim().length > 0) {
    state.chatStreamSegments.push({ text: state.chatStream, ts: Date.now() });
    state.chatStream = null;
    state.chatStreamStartedAt = null;
  }
}

/// Build ChatMessage objects from accumulated tool stream state
function buildToolMessages(state: ChatState): ChatMessage[] {
  return state.toolStreamOrder.map((id) => {
    const tc = state.toolStreamById.get(id);
    if (!tc) {
      return { role: "assistant", content: [{ type: "text", text: "" }], timestamp: Date.now() };
    }
    return {
      role: "assistant",
      toolCallId: tc.toolCallId,
      toolName: tc.toolName,
      content: [
        { type: "toolcall", name: tc.toolName, args: tc.toolArgs ? JSON.parse(tc.toolArgs) : {} },
        ...(tc.toolResult ? [{ type: "toolresult", name: tc.toolName, result: tc.toolResult }] : []),
      ],
      timestamp: tc.startedAt,
    };
  });
}

function extractA2uiSurfaceId(jsonl: string): string | null {
  const lines = jsonl.split("\n").filter((l) => l.trim());
  for (const line of lines) {
    try {
      const msg = JSON.parse(line);
      if (msg.createSurface?.id) return msg.createSurface.id as string;
    } catch { /* skip */ }
  }
  return null;
}
