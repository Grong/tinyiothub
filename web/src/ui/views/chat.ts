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
  private a2uiRenderer = new A2uiRendererEngine();

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
      this.requestUpdate();
    };
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
        </div>
        <div class="chat-main">
          <div class="chat-messages" id="chatMessages">
            ${this.chatState.chatLoading
              ? html`<div class="chat-loading">加载中...</div>`
              : nothing}
            ${groups.map((g) => renderMessageGroup(g))}
            ${this.chatState.chatStream
              ? renderStreamingGroup(
                  this.chatState.chatStream,
                  this.chatState.chatStreamStartedAt || Date.now(),
                )
              : nothing}
            ${this.chatState.chatSending && !this.chatState.chatStream
              ? renderReadingIndicatorGroup()
              : nothing}
            ${this.a2uiRenderer.renderAllSurfaces()}
          </div>
          <div class="chat-input-area">
            <textarea
              class="chat-input"
              .value=${this.draft}
              @input=${(e: Event) => {
                this.draft = (e.target as HTMLTextAreaElement).value;
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
