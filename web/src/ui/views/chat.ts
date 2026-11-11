import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { getWorkspaceId } from '../../api/client.js';
import {
  connectSessionEvents,
  fetchSessionEvents,
  fetchSessionTodos,
  listChatSessions,
  updateSessionPermission,
  type HudSnapshot,
  type SessionSummary,
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
import { renderTimeline } from '../chat/timeline-render.js';
import { A2uiRendererEngine } from '../chat/a2ui/a2ui-renderer.js';
import '../chat/session-drawer.js';
import '../chat/question-card.js';
import '../chat/approval-card.js';
import '../components/global-hud.js';

const AGENT_ID = 'default';

type PermissionMode = 'read_only' | 'ask' | 'full_access';

const SUGGESTIONS = [
  { label: '巡检当前设备', prompt: '巡检当前设备，先看离线和高优告警，再给我结论。' },
  { label: '哪些设备离线了？', prompt: '哪些设备离线了？给出设备和可能原因。' },
  { label: '汇总最近的告警', prompt: '汇总最近的告警，按严重程度排序。' },
  { label: '检查 3 号楼', prompt: '检查 3 号楼现在的状态。' },
];

const KIND_LABEL: Record<string, string> = {
  chat: '对话',
  scheduled: '巡检',
  event: '事件',
};

@customElement('view-chat')
export class ChatView extends LitElement {
  createRenderRoot() {
    return this;
  }

  /** Session selected by a canonical `#/assistant/sessions/{key}` deep link. */
  @property({ type: String }) initialSessionKey = '';
  @state() private sessionKey = '';
  @state() private sessionLabel = '';
  @state() private sessionKind = 'chat';
  @state() private draft = '';
  @state() private liveText = '';
  @state() private thinking = '';
  @state() private sending = false;
  @state() private loading = true;
  @state() private error = '';
  @state() private permissionMode: PermissionMode = 'ask';
  @state() private drawerOpen = false;
  @state() private activeSessions = 0;
  @state() private activeSessionList: SessionSummary[] = [];
  @state() private hud: HudSnapshot | null = null;

  private timeline = new TimelineModel();
  private chatState: ChatState | null = null;
  private closeEvents: (() => void) | null = null;
  private a2uiRenderer = new A2uiRendererEngine((functionId, data) =>
    this.handleA2uiAction(functionId, data),
  );
  private a2uiHydrated = false;
  private stickToBottom = true;
  private threadObserver: ResizeObserver | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    void this.bootstrap();
  }

  disconnectedCallback(): void {
    this.closeEvents?.();
    this.closeEvents = null;
    this.threadObserver?.disconnect();
    this.threadObserver = null;
    super.disconnectedCallback();
  }

  protected firstUpdated(): void {
    const thread = this.querySelector('.assistant-thread');
    if (!thread || typeof ResizeObserver === 'undefined') return;
    this.threadObserver = new ResizeObserver(() => {
      if (!this.stickToBottom) return;
      thread.scrollTop = thread.scrollHeight;
    });
    this.threadObserver.observe(thread);
  }

  private get workspaceId(): string {
    return getWorkspaceId() ?? '';
  }

  private async bootstrap(): Promise<void> {
    const workspaceId = this.workspaceId;
    const stored = localStorage.getItem('tinyiothub_chat_last_session');
    const sessionKey =
      this.initialSessionKey ||
      (stored && stored.startsWith(`agent:${workspaceId}:`)
        ? stored
        : await resolveSessionKey(AGENT_ID));
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
    this.draft = '';
    this.liveText = '';
    this.thinking = '';
    this.error = '';
    this.sending = false;
    this.loading = true;
    this.stickToBottom = true;
    this.permissionMode = 'ask';
    this.sessionKey = sessionKey;
    this.sessionLabel = '';
    this.sessionKind = 'chat';
    localStorage.setItem('tinyiothub_chat_last_session', sessionKey);
    this.chatState = createChatState(sessionKey, AGENT_ID);
    const state = this.chatState;
    this.a2uiRenderer.clear();
    this.a2uiHydrated = false;
    this.chatState.onChange = () => {
      this.liveText = this.chatState?.chatStream ?? '';
      this.thinking = this.chatState?.chatThinking ?? '';
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
    void this.loadSessionMeta();
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
    this.loading = false;
    this.requestUpdate();
  }

  private async send(): Promise<void> {
    const message = this.draft.trim();
    if (!message || !this.chatState || this.sending) return;
    this.draft = '';
    const input = this.querySelector('.composer-input') as HTMLTextAreaElement | null;
    if (input) input.style.height = '';
    this.error = '';
    this.stickToBottom = true;
    sendChatMessage(this.chatState, message);
    this.liveText = '';
    // Pull the persisted user message in case the session SSE is still
    // reconnecting; this does not touch the live chat stream once history
    // has been hydrated.
    void this.loadTimeline(this.sessionKey);
  }

  private useSuggestion(prompt: string): void {
    this.draft = prompt;
    void this.send();
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

  private autoGrow(event: Event): void {
    const textarea = event.target as HTMLTextAreaElement;
    this.draft = textarea.value;
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, 164)}px`;
  }

  private onThreadScroll(): void {
    const thread = this.querySelector('.assistant-thread');
    if (!thread) return;
    this.stickToBottom = thread.scrollHeight - thread.scrollTop - thread.clientHeight < 80;
  }

  private answerPending(): void {
    this.timeline.pendingQuestion = null;
    void this.loadTimeline(this.sessionKey);
  }

  private onApprovalAnswered(event: CustomEvent<string>): void {
    this.timeline.pendingApprovals = this.timeline.pendingApprovals.filter(
      (pending) => pending.approvalId !== event.detail,
    );
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

  private async loadSessionMeta(): Promise<void> {
    try {
      const sessions = await listChatSessions(AGENT_ID);
      const current = sessions.find((session) => session.sessionKey === this.sessionKey);
      if (!current) return;
      this.sessionLabel = current.label ?? '';
      this.sessionKind = current.kind;
      const mode = current.permissionMode;
      if (mode === 'read_only' || mode === 'ask' || mode === 'full_access') {
        this.permissionMode = mode;
      }
    } catch {
      // 读取失败保持当前值；发送/审批仍受后端会话模式约束。
    }
  }

  private async handlePermissionChange(mode: PermissionMode): Promise<void> {
    if (mode === this.permissionMode) return;
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
      (HTMLElement & { refresh(): Promise<void> }) | null;
    void drawer?.refresh();
  }

  private newSessionKey(): string {
    return `agent:${this.workspaceId}:${AGENT_ID}/${crypto.randomUUID()}`;
  }

  private newSession(): void {
    this.selectSession(this.newSessionKey());
    this.drawerOpen = false;
  }

  private onHudUpdate(event: CustomEvent<HudSnapshot>): void {
    this.hud = event.detail;
  }

  private liveSession(): SessionSummary | null {
    return this.activeSessionList.find((session) => session.sessionKey !== this.sessionKey) ?? null;
  }

  private sessionTitleText(): string {
    if (this.sessionLabel) return this.sessionLabel;
    if (this.timeline.firstUserText) {
      return this.timeline.firstUserText.replace(/\s+/g, ' ').slice(0, 28);
    }
    return '新会话';
  }

  protected willUpdate(changed: PropertyValues): void {
    if (!changed.has('initialSessionKey')) return;
    const next = this.initialSessionKey;
    if (next && next !== this.sessionKey) this.selectSession(next);
  }

  protected updated(_changed: PropertyValues): void {
    if (!this.stickToBottom) return;
    const thread = this.querySelector('.assistant-thread');
    if (!thread) return;
    const pin = () => {
      thread.scrollTop = thread.scrollHeight;
    };
    pin();
    requestAnimationFrame(() => {
      pin();
      requestAnimationFrame(pin);
    });
  }

  private renderEmpty() {
    const schedule = this.hud?.schedule;
    const foot = !schedule
      ? '巡检按计划进行'
      : schedule.enabled
        ? schedule.nextWakeAt
          ? `下次巡检 ${new Date(schedule.nextWakeAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })}`
          : '巡检待命'
        : '巡检已暂停';
    return html`
      <section class="assistant-empty" aria-label="开始和大脑一起工作">
        <div class="assistant-empty__eyebrow">大脑待命</div>
        <h1>告诉我这班岗要看什么。</h1>
        <p class="assistant-empty__lead">
          我会按技能和默认职责巡检设备、跟踪告警。涉及写设备时，我会先停下来征求你的批准。
        </p>
        <div class="assistant-empty__chips">
          ${SUGGESTIONS.map(
            (item) => html`
              <button class="empty-chip" @click=${() => this.useSuggestion(item.prompt)}>
                <span>${item.label}</span>
                <span class="empty-chip__arrow" aria-hidden="true">→</span>
              </button>
            `,
          )}
        </div>
        <div class="assistant-empty__foot">${foot} · Enter 发送 · Shift+Enter 换行</div>
      </section>
    `;
  }

  private renderComposer() {
    const fullAccess = this.permissionMode === 'full_access';
    return html`
      <div class="composer-dock">
        <textarea
          class="composer-input chat-input"
          .value=${this.draft}
          placeholder="和大脑一起工作…"
          rows="1"
          ?disabled=${this.sending}
          aria-label="给大脑发消息"
          @input=${(event: Event) => this.autoGrow(event)}
          @keydown=${(event: KeyboardEvent) => this.onKeydown(event)}
        ></textarea>
        <div class="composer-bar">
          <span class="composer-hint"
            >${fullAccess ? '完全访问：免逐次审批，不绕过急停/联锁' : 'Enter 发送 · Shift+Enter 换行'}</span
          >
          <div class="composer-controls">
            <div class="mode-switch permission-mode" role="group" aria-label="会话权限模式">
              <button
                data-mode="read_only"
                aria-pressed=${String(this.permissionMode === 'read_only')}
                title="只读：只查询，不写设备"
                @click=${() => this.handlePermissionChange('read_only')}
              >
                只读
              </button>
              <button
                data-mode="ask"
                aria-pressed=${String(this.permissionMode === 'ask')}
                title="需要审批：读取自动，写设备必须确认"
                @click=${() => this.handlePermissionChange('ask')}
              >
                需审批
              </button>
              <button
                data-mode="full_access"
                aria-pressed=${String(fullAccess)}
                title="完全访问：会话内免逐次审批，但不绕过急停/联锁/Effect"
                @click=${() => this.handlePermissionChange('full_access')}
              >
                完全访问
              </button>
            </div>
            ${
              this.sending
                ? html`<button
                    class="composer-stop chat-abort-btn"
                    aria-label="停止"
                    @click=${() => this.stop()}
                  >
                    停
                  </button>`
                : html`<button
                    class="composer-send chat-send-btn"
                    aria-label="发送"
                    ?disabled=${!this.draft.trim()}
                    @click=${() => this.send()}
                  >
                    ↑
                  </button>`
            }
          </div>
        </div>
      </div>
    `;
  }

  render() {
    const pending: PendingQuestion | null = this.timeline.pendingQuestion;
    const hasContent =
      this.timeline.blocks.length > 0 ||
      this.timeline.todos.length > 0 ||
      Boolean(pending) ||
      this.timeline.pendingApprovals.length > 0 ||
      Boolean(this.liveText);
    const turnActions =
      pending || this.timeline.pendingApprovals.length > 0
        ? html`
            ${
              pending
                ? html`<question-card
                    .sessionKey=${this.sessionKey}
                    .pending=${pending}
                    @answered=${() => this.answerPending()}
                  ></question-card>`
                : nothing
            }
            ${this.timeline.pendingApprovals.map(
              (item) =>
                html`<approval-card
                  .sessionKey=${this.sessionKey}
                  .pending=${item}
                  @answered=${(event: CustomEvent<string>) => this.onApprovalAnswered(event)}
                ></approval-card>`,
            )}
          `
        : undefined;
    return html`
      <div class="assistant-shell">
        <header class="assistant-topbar">
          <button
            class="icon-button"
            aria-label=${this.activeSessions > 0 ? `打开会话列表（${this.activeSessions} 个运行中）` : '打开会话列表'}
            @click=${() => (this.drawerOpen = true)}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M2 4h12M2 8h12M2 12h8"
                stroke="currentColor"
                stroke-width="1.4"
                fill="none"
              />
            </svg>
            ${this.activeSessions > 0 ? html`<span class="icon-button__dot" aria-hidden="true"></span>` : nothing}
          </button>
          <div class="assistant-topbar__identity">
            <span class="assistant-topbar__title">${this.sessionTitleText()}</span>
            <span class="assistant-topbar__kind"
              >${KIND_LABEL[this.sessionKind] ?? this.sessionKind}</span
            >
          </div>
          <span class="assistant-topbar__spacer"></span>
          ${
            this.liveSession()
              ? html`<button
                  class="assistant-live"
                  aria-label="查看正在运行的会话"
                  title=${this.liveSession()!.label ?? '正在运行的会话'}
                  @click=${() => this.selectSession(this.liveSession()!.sessionKey)}
                >
                  <span class="assistant-live__dot" aria-hidden="true"></span>
                  <span>正在运行</span>
                  <span class="assistant-live__title"
                    >${this.liveSession()!.label ?? '查看执行过程'}</span
                  >
                </button>`
              : nothing
          }
          <button class="assistant-topbar__new" @click=${() => this.newSession()}>
            <span>新会话</span><span aria-hidden="true">＋</span>
          </button>
        </header>
        <global-hud .workspaceId=${this.workspaceId} @hud-update=${this.onHudUpdate}></global-hud>
        <div class="assistant-thread" @scroll=${() => this.onThreadScroll()}>
          ${
            !hasContent && !this.sending && !this.loading
              ? this.renderEmpty()
              : !hasContent && this.loading
                ? html`<div class="assistant-empty__foot">正在读取会话…</div>`
                : html`
                    ${renderTimeline(this.timeline.parts, {
                      liveText: this.liveText,
                      thinking: this.sending ? this.thinking : '',
                      sending: this.sending,
                      todos: this.timeline.todos,
                      waitingForUser: Boolean(pending) || this.timeline.pendingApprovals.length > 0,
                      turnActions,
                    })}
                    ${this.a2uiRenderer.renderSurfacesByKind('inline')}
                    ${this.a2uiRenderer.renderSurfacesByKind('overlay')}
                    ${
                      this.error
                        ? html`<div class="timeline-notice timeline-notice--timed_out">
                            ${this.error}
                          </div>`
                        : ''
                    }
                  `
          }
        </div>
        ${this.renderComposer()}
        <session-drawer
          .agentId=${AGENT_ID}
          .current=${this.sessionKey}
          .open=${this.drawerOpen}
          @drawer-close=${() => (this.drawerOpen = false)}
          @session-select=${(event: CustomEvent<string>) => this.selectSession(event.detail)}
          @session-new=${() => this.newSession()}
          @session-deleted=${(event: CustomEvent<string>) => {
            if (event.detail === this.sessionKey) this.newSession();
            else this.refreshDrawer();
          }}
          @sessions-updated=${(
            event: CustomEvent<{ active: number; sessions: SessionSummary[] }>,
          ) => {
            this.activeSessions = event.detail.active;
            this.activeSessionList = event.detail.sessions;
          }}
        ></session-drawer>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-chat': ChatView;
  }
}
