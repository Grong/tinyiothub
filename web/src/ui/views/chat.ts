import { LitElement, html, nothing } from "lit";
import { customElement, state } from "lit/decorators.js";
import type { ChatState } from "../controllers/chat.js";
import {
  createChatState,
  loadChatHistory,
  sendChatMessage,
  abortChatRun,
} from "../controllers/chat.js";
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
  @state() sessionsList: Array<{ key: string; label: string }> = [];
  @state() sessionKey: string = "";
  @state() sidebarCollapsed: boolean = false;
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
    const defaultKey = crypto.randomUUID();
    this.sessionKey = defaultKey;
    this.agentId = "default"; // TODO: get from URL params or store
    this.sessionsList = [{ key: defaultKey, label: "新会话" }];
    this.chatState = createChatState(this.sessionKey, this.agentId);
    this._bindA2uiCallback();
    loadChatHistory(this.chatState).then(() => this.requestUpdate());
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this._stopStreamPolling();
  }

  private switchSession(key: string): void {
    this.sessionKey = key;
    this.chatState = createChatState(key, this.agentId);
    this.a2uiRenderer.clear();
    this._bindA2uiCallback();
    loadChatHistory(this.chatState).then(() => {
      this.requestUpdate();
      this.scrollToBottom();
    });
  }

  private handleSend(): void {
    const msg = this.draft.trim();
    if (!msg) return;
    this.draft = "";
    sendChatMessage(this.chatState, msg);
    this.requestUpdate();
    this._startStreamPolling();

    // Auto-title: use first 20 chars of first user message
    const sessionIdx = this.sessionsList.findIndex((s) => s.key === this.sessionKey);
    if (sessionIdx >= 0 && this.sessionsList[sessionIdx].label === "新会话") {
      const title = msg.length > 20 ? msg.slice(0, 20) + "..." : msg;
      const updated = [...this.sessionsList];
      updated[sessionIdx] = { ...updated[sessionIdx], label: title };
      this.sessionsList = updated;
    }
  }

  private handleAbort(): void {
    abortChatRun(this.chatState);
  }

  private handleNewSession(): void {
    const newKey = crypto.randomUUID();
    this.sessionKey = newKey;
    this.chatState = createChatState(newKey, this.agentId);
    this.a2uiRenderer.clear();
    this._bindA2uiCallback();
    this.sessionsList = [
      ...this.sessionsList,
      { key: newKey, label: "新会话" },
    ];
    this.requestUpdate();
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
        <div
          class="chat-sidebar ${this.sidebarCollapsed ? "collapsed" : ""}"
        >
          <button class="chat-sidebar-toggle"
                  @click=${() => { this.sidebarCollapsed = !this.sidebarCollapsed; }}>
            ${this.sidebarCollapsed ? "▶" : "◀"}
          </button>
          ${!this.sidebarCollapsed ? html`
            <button
              class="chat-new-session-btn"
              @click=${this.handleNewSession}
            >
              新建会话
            </button>
            ${this.sessionsList.map(
              (s) => html`
                <div
                  class="chat-session-item ${s.key === this.sessionKey
                    ? "active"
                    : ""}"
                  @click=${() => this.switchSession(s.key)}
                >
                  ${s.label}
                </div>
              `,
            )}
          ` : nothing}
        </div>
        <div class="chat-main">
          <div class="chat-messages" id="chatMessages">
            ${this.chatState.chatLoading
              ? html`<div class="chat-loading">加载中...</div>`
              : nothing}
            ${groups.map((g) => renderMessageGroup(g, this.a2uiRenderer))}
            ${this.chatState.chatStream
              ? renderStreamingGroup(
                  this.chatState.chatStream,
                  this.chatState.chatStreamStartedAt || Date.now(),
                )
              : nothing}
            ${this.chatState.chatSending && !this.chatState.chatStream
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
              ? html`<button
                  class="chat-abort-btn"
                  @click=${this.handleAbort}
                >
                  停止
                </button>`
              : html`<button
                  class="chat-send-btn"
                  @click=${this.handleSend}
                  ?disabled=${!this.draft.trim()}
                >
                  发送
                </button>`}
          </div>
        </div>
      </div>
    `;
  }
}
