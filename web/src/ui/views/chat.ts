import { LitElement, html, type PropertyValues } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { getWorkspaceId } from '../../api/client.js';
import {
  connectSessionEvents,
  fetchSessionEvents,
  fetchSessionTodos,
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
      stored && stored.startsWith(`agent:${workspaceId}:`)
        ? stored
        : await resolveSessionKey(AGENT_ID);
    localStorage.setItem('tinyiothub_chat_last_session', sessionKey);
    this.selectSession(sessionKey);
  }

  private selectSession(sessionKey: string): void {
    this.closeEvents?.();
    this.closeEvents = null;
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
        this.requestUpdate();
      },
      onError: (error) => {
        this.error = error.message;
        this.requestUpdate();
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
        this.timeline.blocks = timelineFromHistory(this.chatState.chatMessages);
      }
      this.error = '';
    } catch (error) {
      this.error = (error as Error).message || '加载会话失败';
    }
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

  private refreshDrawer(): void {
    const drawer = this.querySelector('session-drawer') as
      | (HTMLElement & { refresh(): Promise<void> })
      | null;
    void drawer?.refresh();
  }

  private async newSession(): Promise<void> {
    this.selectSession(await resolveSessionKey(AGENT_ID));
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
          @session-new=${() => void this.newSession()}
          @session-deleted=${(event: CustomEvent<string>) => {
            if (event.detail === this.sessionKey) void this.newSession();
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
              <span class="codex-mode" title="权限模式接线在 Plan 4">需要审批</span>
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
