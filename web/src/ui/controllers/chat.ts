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
  onA2ui?: (jsonl: string) => void;
  lastA2uiSurfaceId?: string;
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
  const token = sessionStorage.getItem("auth-token") || localStorage.getItem("auth-token") || "";
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
      if (!response.ok) {
        let errMsg = `HTTP ${response.status}`;
        try {
          const errData = await response.json();
          errMsg = errData?.msg || errData?.message || errMsg;
        } catch {
          // not JSON
        }
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

  if (payload.a2ui && state.onA2ui) {
    // Track the last surface ID from A2UI messages
    const surfaceId = extractA2uiSurfaceId(payload.a2ui);
    if (surfaceId) {
      state.lastA2uiSurfaceId = surfaceId;
    }
    state.onA2ui(payload.a2ui);
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

function extractA2uiSurfaceId(jsonl: string): string | null {
  const lines = jsonl.split("\n").filter((l) => l.trim());
  for (const line of lines) {
    try {
      const msg = JSON.parse(line);
      if (msg.createSurface?.id) return msg.createSurface.id as string;
    } catch {
      // skip non-JSON lines
    }
  }
  return null;
}
