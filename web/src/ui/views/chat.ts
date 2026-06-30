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
  setChipToggleCallback,
} from "../chat/grouped-render.js";
import { A2uiRendererEngine } from "../chat/a2ui/a2ui-renderer.js";
import { resolveSessionKey } from "../shared/session-key.js";

@customElement("view-chat")
export class ChatView extends LitElement {
  @state() chatState: ChatState = createChatState("", "");
  @state() draft: string = "";
  @state() agentId: string = "";

  private a2uiRenderer = new A2uiRendererEngine((functionId: string, data: Record<string, unknown>) => {
    this._handleA2uiAction(functionId, data);
  });

  createRenderRoot() {
    return this;
  }

  async connectedCallback(): Promise<void> {
    super.connectedCallback();
    this.agentId = "default"; // TODO: get from URL params or store
    // Register chip toggle callback for Lit re-render
    setChipToggleCallback(() => this.requestUpdate());

    const sessionKey = await resolveSessionKey(this.agentId);

    this.chatState = createChatState(sessionKey, this.agentId);
    // Event-driven re-render — no polling
    this.chatState.onChange = () => {
      this.requestUpdate();
      this.scrollToBottom();
    };
    this._bindA2uiCallback();
    await loadChatHistory(this.chatState);
    // Re-hydrate A2UI surfaces from history
    for (const msg of this.chatState.chatMessages) {
      const a2ui = (msg as Record<string, unknown>).a2ui as string | undefined;
      if (a2ui) {
        this.a2uiRenderer.handleA2uiMessage(a2ui);
      }
    }
    this.requestUpdate();
  }

  private handleSend(): void {
    const msg = this.draft.trim();
    if (!msg) return;
    this.draft = "";
    sendChatMessage(this.chatState, msg);
    this.requestUpdate();
  }

  private handleAbort(): void {
    abortChatRun(this.chatState);
  }

  private _bindA2uiCallback(): void {
    this.chatState.onA2ui = (jsonl: string) => {
      this.a2uiRenderer.handleA2uiMessage(jsonl);
      this.requestUpdate();
    };
  }

  // @ts-expect-error — kept for future A2UI surface attachment flow
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
    const deviceId = data.deviceId as string | undefined;

    // Navigation actions — go directly to the page, don't send a chat message
    if (functionId === "viewDevice" && deviceId) {
      window.history.pushState({}, "", `/devices/${deviceId}`);
      window.dispatchEvent(new PopStateEvent("popstate"));
      return;
    }
    if (functionId === "controlDevice" && deviceId) {
      window.history.pushState({}, "", `/devices/${deviceId}`);
      window.dispatchEvent(new PopStateEvent("popstate"));
      return;
    }

    // Other A2UI actions — send as chat message for the agent to handle
    const actionMsg = `[操作] ${functionId}: ${JSON.stringify(data)}`;
    sendChatMessage(this.chatState, actionMsg);
    this.requestUpdate();
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

  private _suggestions = [
    { icon: 'M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z', text: '查看所有设备状态' },
    { icon: 'M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01', text: '最近有哪些告警' },
    { icon: 'M18 20V10M12 20V4M6 20v-6', text: '分析系统运行状况' },
    { icon: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z', text: '如何添加新设备' },
    { icon: 'M5 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M19 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M12 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4z M7 7l5 8 M17 7l-5 8', text: '管理知识图谱' },
    { icon: 'M22 12h-4l-3 9L9 3l-3 9H2', text: '查看驱动健康状态' },
  ];

  private _sendSuggestion(text: string): void {
    this.draft = text;
    this.handleSend();
  }

  private _renderWelcome() {
    return html`
      <div class="chat-welcome">
        <div class="chat-welcome__icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
          </svg>
        </div>
        <h2 class="chat-welcome__title">AI 助手</h2>
        <p class="chat-welcome__desc">我是您的 IoT 智能助手，可以帮助您管理设备、分析数据、处理告警等。请选择下方问题或直接输入内容开始对话。</p>
        <div class="chat-suggestions">
          ${this._suggestions.map(
            (s) => html`
              <button class="chat-suggestion-chip" @click=${() => this._sendSuggestion(s.text)}>
                <span class="chat-suggestion-chip__icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="${s.icon}"/>
                  </svg>
                </span>
                <span class="chat-suggestion-chip__text">${s.text}</span>
              </button>
            `,
          )}
        </div>
      </div>
    `;
  }

  render(): ReturnType<typeof html> {
    const groups = groupMessages(this.chatState.chatMessages);
    const showWelcome = !this.chatState.chatLoading && this.chatState.chatMessages.length === 0 && !this.chatState.chatSending;

    return html`
      <div class="chat-layout">
        <div class="chat-messages" id="chatMessages">
          ${this.chatState.chatLoading ? html`<div style="padding: 20px; text-align: center; color: var(--muted); font-size: 13px;">加载中...</div>` : nothing}
          ${showWelcome ? this._renderWelcome() : nothing}
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
