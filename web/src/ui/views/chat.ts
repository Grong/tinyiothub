import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { ChatState } from "../controllers/chat.js";
import {
  createChatState,
  loadChatHistory,
  sendChatMessage,
  abortChatRun,
} from "../controllers/chat.js";
import { apiGet } from "../../api/client.js";
import {
  groupMessages,
  renderMessageGroup,
  renderStreamingGroup,
  renderReadingIndicatorGroup,
} from "../chat/grouped-render.js";
import { A2uiRendererEngine } from "../chat/a2ui/a2ui-renderer.js";

@customElement("view-chat")
export class ChatView extends LitElement {
  @state() chatState: ChatState = createChatState("", "");
  @state() draft: string = "";
  @state() agentId: string = "";

  private _pollTimer: ReturnType<typeof setInterval> | null = null;
  private a2uiRenderer = new A2uiRendererEngine((functionId: string, data: Record<string, unknown>) => {
    this._handleA2uiAction(functionId, data);
  });

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.agentId = "default"; // TODO: get from URL params or store
    // Persist session key so chat history loads correctly across page reloads
    // Format: agent:<workspace_id>:<agent_id>/<session_uuid>
    const storedKey = localStorage.getItem("tinyiothub_chat_session_key");
    let sessionKey = storedKey;
    if (!storedKey || !storedKey.includes('/')) {
      sessionKey = `agent:default:${this.agentId}/${crypto.randomUUID()}`;
      localStorage.setItem("tinyiothub_chat_session_key", sessionKey);
    }
    // Load agent config to get systemPrompt, then create chat state
    apiGet<{ config: { systemPrompt?: string } }>(`/agents/${this.agentId}/config`)
      .then((res) => {
        const systemPrompt = res.result?.config?.systemPrompt;
        this.chatState = createChatState(sessionKey || "", this.agentId, systemPrompt);
        this._bindA2uiCallback();
        loadChatHistory(this.chatState).then(() => this.requestUpdate());
      })
      .catch(() => {
        // ZeroClaw not connected or config unavailable — still allow chat
        this.chatState = createChatState(sessionKey || "", this.agentId);
        this._bindA2uiCallback();
        loadChatHistory(this.chatState).then(() => this.requestUpdate());
      });
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this._stopStreamPolling();
  }

  private handleSend(): void {
    const msg = this.draft.trim();
    if (!msg) return;
    this.draft = "";
    sendChatMessage(this.chatState, msg);
    this.requestUpdate();
    this._startStreamPolling();
  }

  private handleAbort(): void {
    abortChatRun(this.chatState);
  }

  private _bindA2uiCallback(): void {
    this.chatState.onA2ui = (jsonl: string) => {
      this.a2uiRenderer.handleA2uiMessage(jsonl);
      this._attachLastSurfaceToMessage();
      this.requestUpdate();
    };
  }

  private _attachLastSurfaceToMessage(): void {
    const surfaceId = this.chatState.lastA2uiSurfaceId;
    if (!surfaceId) return;
    const msgs = this.chatState.chatMessages;
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === "assistant") {
        const updated = [...msgs];
        updated[i] = { ...updated[i], a2uiSurfaceId: surfaceId } as any;
        this.chatState.chatMessages = updated;
        return;
      }
    }
  }

  private _handleA2uiAction(functionId: string, data: Record<string, unknown>): void {
    const actionMsg = `[操作] ${functionId}: ${JSON.stringify(data)}`;
    sendChatMessage(this.chatState, actionMsg);
    this._startStreamPolling();
  }

  private _startStreamPolling(): void {
    this._stopStreamPolling();
    this._pollTimer = setInterval(() => {
      this.requestUpdate();
      this.scrollToBottom();
      if (!this.chatState.chatSending) {
        this._stopStreamPolling();
      }
    }, 100);
  }

  private _stopStreamPolling(): void {
    if (this._pollTimer) {
      clearInterval(this._pollTimer);
      this._pollTimer = null;
    }
  }

  private scrollToBottom(): void {
    const el = this.querySelector("#chatMessages");
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }

  updated(): void {
    this.scrollToBottom();
  }

  render(): ReturnType<typeof html> {
    const groups = groupMessages(this.chatState.chatMessages);

    return html`
      <div class="chat-layout">
        <div class="chat-messages" id="chatMessages">
          ${this.chatState.chatLoading ? html`<div style="padding: 20px; text-align: center; color: var(--muted); font-size: 13px;">加载中...</div>` : nothing}
          ${groups.map((g) => renderMessageGroup(g, this.a2uiRenderer))}
          ${this.chatState.chatSending && (this.chatState.chatStream !== null || this.chatState.toolStreamOrder.length > 0)
            ? renderStreamingGroup(
                this.chatState.chatStreamSegments,
                this.chatState.toolStreamOrder,
                this.chatState.toolStreamById,
                this.chatState.chatStream || "",
                this.a2uiRenderer,
              )
            : nothing}
          ${this.chatState.chatSending && !this.chatState.chatStream && this.chatState.toolStreamOrder.length === 0
            ? renderReadingIndicatorGroup()
            : nothing}
        </div>
        <div class="chat-input-area">
          <textarea
            class="chat-input"
            .value=${this.draft}
            @input=${(e: Event) => {
              const ta = e.target as HTMLTextAreaElement;
              this.draft = ta.value;
              ta.style.height = "auto";
              ta.style.height = Math.min(ta.scrollHeight, 120) + "px";
            }}
            @keydown=${(e: KeyboardEvent) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                this.handleSend();
              }
            }}
            placeholder="输入消息..."
          ></textarea>
          ${this.chatState.chatSending
            ? html`<button class="chat-abort-btn" @click=${this.handleAbort}>停止</button>`
            : html`<button class="chat-send-btn" @click=${this.handleSend} ?disabled=${!this.draft.trim()}>发送</button>`}
        </div>
      </div>
    `;
  }
}
