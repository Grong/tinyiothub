import { LitElement, html, type PropertyValues } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { getWorkspaceId } from '../../api/client.js';
import {
  connectSessionEvents,
  fetchSessionEvents,
  fetchSessionTodos,
  listChatSessions,
  updateSessionPermission,
} from '../../api/assistant.js';
import {
  abortChatRun,
  createChatState,
  loadChatHistory,
  sendChatMessage,
  type ChatState,
} from '../controllers/chat.js';
import { resolveSessionKey } from '../shared/session-key.js';
import {
  TimelineModel,
  timelineFromHistory,
  type PendingQuestion,
} from '../chat/timeline-model.js';
import { renderTimeline, renderTodoPanel } from '../chat/timeline-render.js';
import { A2uiRendererEngine } from '../chat/a2ui/a2ui-renderer.js';
import '../chat/session-drawer.js';
import '../chat/question-card.js';
import '../chat/approval-card.js';
import '../components/global-hud.js';

const AGENT_ID = 'default';

type PermissionMode = 'read_only' | 'ask' | 'full_access';

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
  @state() private permissionMode: PermissionMode = 'ask';

  private timeline = new TimelineModel();
  private chatState: ChatState | null = null;
  private closeEvents: (() => void) | null = null;
  private a2uiRenderer = new A2uiRendererEngine((functionId, data) =>
    this.handleA2uiAction(functionId, data),
  );
  private a2uiHydrated = false;

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
      stored && stored.startsWith(`agent:${workspaceId}:`)
        ? stored
        : await resolveSessionKey(AGENT_ID);
    localStorage.setItem('tinyiothub_chat_last_session', sessionKey);
    this.selectSession(sessionKey);
  }

  private selectSession(sessionKey: string): void {
    this.closeEvents?.();
    this.closeEvents = null;
    if (this.chatState?.chatSending) {
      void abortChatRun(this.chatState);
    }
    if (this.chatState) {
      this.chatState.onChange = () => {};
      this.chatState.onA2ui = () => {};
    }
    this.timeline.reset();
    this.liveText = '';
    this.error = '';
    this.sending = false;
    this.permissionMode = 'ask';
    this.sessionKey = sessionKey;
    localStorage.setItem('tinyiothub_chat_last_session', sessionKey);
    this.chatState = createChatState(sessionKey, AGENT_ID);
    const state = this.chatState;
    this.a2uiRenderer.clear();
    this.a2uiHydrated = false;
    this.chatState.onChange = () => {
      this.liveText = this.chatState?.chatStream ?? '';
      this.sending = Boolean(this.chatState?.chatSending);
      this.error = this.chatState?.lastError ?? '';
      this.requestUpdate();
    };
    state.onA2ui = (jsonl: string) => {
      if (this.chatState !== state) return;
      this.a2uiRenderer.handleA2uiMessage(jsonl);
      this.requestUpdate();
    };
    this.refreshDrawer();
    void this.loadPermissionMode();
    void this.loadTimeline(sessionKey);
    this.closeEvents = connectSessionEvents({
      sessionKey,
      after: 0,
      onEvent: (event) => {
        if (this.sessionKey !== sessionKey) return;
        this.timeline.ingestEvent(event);
        this.requestUpdate();
      },
      onError: (error) => {
        if (this.sessionKey !== sessionKey) return;
        this.error = error.message;
        this.requestUpdate();
      },
    });
  }

  private async loadTimeline(sessionKey: string): Promise<void> {
    const key = sessionKey;
    const chatState = this.chatState;
    try {
      let pageCursor = 0;
      let page = await fetchSessionEvents(key, pageCursor, 200);
      if (key !== this.sessionKey) return;
      this.timeline.ingest(page);
      for (const event of page.events) pageCursor = Math.max(pageCursor, event.seq);
      let guard = 0;
      while (pageCursor < page.highWatermark && guard < 10) {
        page = await fetchSessionEvents(key, pageCursor, 200);
        if (key !== this.sessionKey) return;
        if (page.events.length === 0) break;
        this.timeline.ingest(page);
        for (const event of page.events) pageCursor = Math.max(pageCursor, event.seq);
        guard += 1;
      }
      const todos = await fetchSessionTodos(key);
      if (key !== this.sessionKey) return;
      if (todos.length > 0 && this.timeline.todos.length === 0) this.timeline.todos = todos;
      if (chatState && !this.a2uiHydrated) {
        await loadChatHistory(chatState);
        if (key !== this.sessionKey) return;
        for (const message of chatState.chatMessages as unknown as Array<Record<string, unknown>>) {
          const a2ui = message.a2ui;
          if (typeof a2ui === 'string' && a2ui) this.a2uiRenderer.handleA2uiMessage(a2ui);
        }
        this.a2uiHydrated = true;
        if (this.timeline.blocks.length === 0) {
          this.timeline.blocks = timelineFromHistory(
            chatState.chatMessages as unknown as Array<{
              role: string;
              content?: Array<{ type: string; text?: string }>;
            }>,
          );
        }
      }
      this.error = '';
    } catch (error) {
      if (key !== this.sessionKey) return;
      this.error = (error as Error).message || '加载会话失败';
    }
    if (key !== this.sessionKey) return;
    this.requestUpdate();
  }

  private async send(): Promise<void> {
    const message = this.draft.trim();
    if (!message || !this.chatState || this.sending) return;
    this.draft = '';
    this.error = '';
    sendChatMessage(this.chatState, message);
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

  private onApprovalAnswered(): void {
    this.timeline.pendingApproval = null;
    void this.loadTimeline(this.sessionKey);
  }

  private handleA2uiAction(functionId: string, data: Record<string, unknown>): void {
    const thingId = (data.thingId ?? data.deviceId) as string | undefined;
    if ((functionId === 'viewDevice' || functionId === 'controlDevice') && thingId) {
      window.history.pushState({}, '', `/things/${thingId}`);
      window.dispatchEvent(new PopStateEvent('popstate'));
      return;
    }
    if (this.chatState) {
      void sendChatMessage(this.chatState, `[操作] ${functionId}: ${JSON.stringify(data)}`);
    }
    this.requestUpdate();
  }

  private async loadPermissionMode(): Promise<void> {
    try {
      const sessions = await listChatSessions(AGENT_ID);
      const current = sessions.find((session) => session.sessionKey === this.sessionKey);
      const mode = current?.permissionMode;
      if (mode === 'read_only' || mode === 'ask' || mode === 'full_access') {
        this.permissionMode = mode;
      }
    } catch {
      // 读取失败保持当前值；发送/审批仍受后端会话模式约束。
    }
  }

  private async handlePermissionChange(mode: PermissionMode): Promise<void> {
    const key = this.sessionKey;
    const previous = this.permissionMode;
    this.permissionMode = mode;
    try {
      await updateSessionPermission(key, mode);
      if (this.sessionKey !== key) return;
      this.refreshDrawer();
    } catch (error) {
      if (this.sessionKey !== key) return;
      this.permissionMode = previous;
      this.error = (error as Error).message || '保存权限模式失败';
    }
  }

  private refreshDrawer(): void {
    const drawer = this.querySelector('session-drawer') as
      | (HTMLElement & { refresh(): Promise<void> })
      | null;
    void drawer?.refresh();
  }

  private newSessionKey(): string {
    return `agent:${this.workspaceId}:${AGENT_ID}/${crypto.randomUUID()}`;
  }

  private newSession(): void {
    this.selectSession(this.newSessionKey());
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
          @session-new=${() => this.newSession()}
          @session-deleted=${(event: CustomEvent<string>) => {
            if (event.detail === this.sessionKey) this.newSession();
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
            ${this.timeline.pendingApproval
              ? html`<approval-card
                  .sessionKey=${this.sessionKey}
                  .pending=${this.timeline.pendingApproval}
                  @answered=${() => this.onApprovalAnswered()}
                ></approval-card>`
              : ''}
            ${renderTimeline(this.timeline.blocks, { liveText: this.liveText, sending: this.sending })}
            ${this.a2uiRenderer.renderSurfacesByKind('inline')}
            ${this.a2uiRenderer.renderSurfacesByKind('overlay')}
            ${this.error
              ? html`<div class="timeline-notice timeline-notice--timed_out">${this.error}</div>`
              : ''}
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
              <select
                class="permission-mode"
                .value=${this.permissionMode}
                aria-label="会话权限模式"
                title="会话权限模式"
                @change=${(event: Event) =>
                  this.handlePermissionChange(
                    (event.target as HTMLSelectElement).value as PermissionMode,
                  )}
              >
                <option value="read_only">只读</option>
                <option value="ask">需要审批</option>
                <option value="full_access">完全访问</option>
              </select>
              <span
                class="codex-mode-hint"
                title="完全访问只免除逐次审批，不绕过急停/联锁/Effect"
              >?</span>
              ${this.sending
                ? html`<button class="chat-abort-btn" @click=${() => this.stop()}>停止</button>`
                : html`<button
                    class="chat-send-btn"
                    ?disabled=${!this.draft.trim()}
                    @click=${() => this.send()}
                  >
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
