import { describe, expect, it } from "vitest";
import { handleChatEvent, type ChatState } from "./chat.js";

function makeState(): ChatState {
  return {
    sessionKey: "agent:ws:a1/s1",
    agentId: "a1",
    chatLoading: false,
    chatMessages: [],
    chatSending: true,
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

describe("handleChatEvent runId adoption", () => {
  it("adopts the server-generated runId from the first event", () => {
    // The server mints run_id; the client must latch onto the id carried by
    // the first SSE event, otherwise every event is filtered as cross-run
    // and abort targets a run the server never knew.
    const state = makeState();
    handleChatEvent(state, {
      runId: "server-run-1",
      sessionKey: "agent:ws:a1/s1",
      state: "delta",
      message: { role: "assistant", content: [{ type: "text", text: "hi" }] },
    });
    expect(state.chatRunId).toBe("server-run-1");
    expect(state.chatStream).toBe("hi");
  });

  it("does not overwrite the adopted runId on later events", () => {
    const state = makeState();
    state.chatRunId = "server-run-1";
    handleChatEvent(state, {
      runId: "other-run",
      sessionKey: "agent:ws:a1/s1",
      state: "delta",
      message: { role: "assistant", content: [{ type: "text", text: "x" }] },
    });
    expect(state.chatRunId).toBe("server-run-1");
    expect(state.chatStream).toBeNull();
  });
});
