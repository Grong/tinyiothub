/**
 * AI 工作纪实 — 给领导/普通用户看的 harness 过程页。
 *
 * 左栏把 session_events 翻译成业务叙事（看了哪些设备、发现了什么、怎么处置、
 * 结论是什么），右栏用卡牌堆展示涉及设备、现场画面和 AI 大脑今日/本周功绩。
 *
 * 工程师视图（原始工具/思考流）仍然在 AI 助手里，这里只做“看得懂”的投影。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import { unsafeHTML } from 'lit/directives/unsafe-html.js';
import type { TodoItem } from '../../api/assistant.js';
import {
  connectSessionEvents,
  fetchSessionEvents,
  fetchSessionProjection,
  fetchSessionTodos,
  listChatSessions,
  type SessionEvent,
  type SessionSummary,
} from '../../api/assistant.js';
import { getWorkspaceId } from '../../api/client.js';
import { brainEventApi, type BrainEvent, type BrainEventsSummary } from '../../api/brain-events.js';
import { TimelineModel } from '../chat/timeline-model.js';
import { md } from '../shared/markdown.js';
import {
  buildStory,
  EMPTY_STORY,
  failureCopy,
  formatStoryClock,
  STORY_ORIGIN_LABEL,
  type StorySection,
  type StoryStep,
  type StoryView as StoryProjection,
} from '../story/story-model.js';
import {
  computeAchievements,
  EMPTY_ACHIEVEMENTS,
  type AchievementStats,
} from '../story/story-achievements.js';
import '../story/achievements-panel.js';
import '../chat/task-plan-panel.js';
import '../chat/approval-card.js';
import '../chat/question-card.js';
import '../story/device-deck.js';
import '../story/vision-deck.js';
import '../../styles/views/story.css';

const KIND_LABEL: Record<string, string> = {
  chat: '对话',
  scheduled: '巡检',
  event: '事件处置',
};

const SECTION_STATUS: Record<string, string> = {
  running: '进行中',
  done: '已完成',
  failed: '执行失败',
  cancelled: '已取消',
  partial: '部分完成',
};

const STEP_STATUS: Record<StoryStep['status'], string> = {
  running: '进行中',
  done: '完成',
  failed: '失败',
};

const STEP_ICON: Record<StoryStep['tone'], ReturnType<typeof html>> = {
  read: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/></svg>`,
  write: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>`,
  alarm: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>`,
  plan: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>`,
  ask: html`<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.1 9a3 3 0 1 1 5.8 1c0 2-3 2-3 4"/><path d="M12 17h.01"/></svg>`,
  analysis: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12h4l3 8 4-16 3 8h4"/></svg>`,
  skill: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 2 22 12 12 22 2 12Z"/></svg>`,
  think: html`<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M12 2a7 7 0 0 0-4 12.7V17h8v-2.3A7 7 0 0 0 12 2Z"/></svg>`,
};

const ASSISTANT_STATUS: Record<string, string> = {
  idle: '待命',
  working: '正在工作',
  waiting_for_user: '等你确认',
  paused: '已暂停',
  error: '异常',
};

function formatSessionTime(ms: number): string {
  const date = new Date(ms);
  if (!Number.isFinite(date.getTime())) return '—';
  const clock = date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
  return `${date.getMonth() + 1}月${date.getDate()}日 ${clock}`;
}

function clampText(value: string, max: number): string {
  const text = value.replace(/\s+/g, ' ').trim();
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

/** 默认记录：最近更新的会话；更新时间相同才优先 active。 */
export function pickDefaultSession(sessions: SessionSummary[]): SessionSummary | null {
  if (sessions.length === 0) return null;
  return [...sessions].sort(
    (left, right) =>
      right.updatedAt - left.updatedAt ||
      (right.status === 'active' ? 1 : 0) - (left.status === 'active' ? 1 : 0),
  )[0];
}

@customElement('view-story')
export class StoryView extends LitElement {
  createRenderRoot() {
    return this;
  }

  @state() private sessions: SessionSummary[] = [];
  @state() private sessionKey = '';
  @state() private story: StoryProjection = EMPTY_STORY;
  @state() private loading = true;
  @state() private error = '';
  @state() private achievements: AchievementStats = EMPTY_ACHIEVEMENTS;
  @state() private achievementsLoading = false;
  @state() private historyBefore: number | null = null;
  @state() private historyHasMore = false;
  @state() private loadingOlder = false;

  private timeline = new TimelineModel();
  private closeEvents: (() => void) | null = null;
  private refreshTimer: number | null = null;
  private loadSeq = 0;
  private brainEvents: BrainEvent[] = [];
  private brainSummary: BrainEventsSummary | null = null;
  private manualSelection = false;
  private historyLocked = false;
  @state() private stickToLatest = true;
  private manualScroll = false;
  private historyEpoch = 0;
  private pinPending = false;
  private suppressScroll = false;
  private feedObserver: ResizeObserver | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    void this.bootstrap();
  }

  disconnectedCallback(): void {
    this.closeEvents?.();
    this.closeEvents = null;
    if (this.refreshTimer !== null) window.clearInterval(this.refreshTimer);
    this.refreshTimer = null;
    this.feedObserver?.disconnect();
    this.feedObserver = null;
    super.disconnectedCallback();
  }

  protected firstUpdated(): void {
    const feed = this.querySelector('.story-feed');
    if (!feed || typeof ResizeObserver === 'undefined') return;
    this.feedObserver = new ResizeObserver(() => {
      if (this.stickToLatest && !this.loadingOlder && !this.suppressScroll) {
        this.pinFeedToBottom();
      }
    });
    this.feedObserver.observe(feed);
  }

  private get workspaceId(): string {
    return getWorkspaceId() ?? '';
  }

  private async bootstrap(): Promise<void> {
    if (!this.workspaceId) {
      this.error = '缺少工作区上下文，请先登录并选择工作区。';
      this.loading = false;
      return;
    }
    try {
      const sessions = await listChatSessions();
      this.sessions = sessions;
      const selected = pickDefaultSession(sessions);
      if (selected) await this.selectSession(selected.sessionKey);
      else this.loading = false;
    } catch (error) {
      this.error = (error as Error).message || '加载工作记录失败';
      this.loading = false;
    }
    void this.loadDashboard();
    this.refreshTimer = window.setInterval(() => {
      void this.loadDashboard();
    }, 60_000);
  }

  private async loadDashboard(): Promise<void> {
    this.achievementsLoading = true;
    try {
      const [summary, events, sessions] = await Promise.all([
        brainEventApi.summary().catch(() => null),
        this.loadBrainEvents(),
        listChatSessions().catch(() => null),
      ]);
      this.brainSummary = summary;
      this.brainEvents = events;
      if (sessions) {
        const previous = this.sessions;
        this.sessions = sessions;
        const preferred = pickDefaultSession(sessions);
        const current = previous.find((session) => session.sessionKey === this.sessionKey);
        if (
          !this.manualSelection &&
          preferred &&
          preferred.sessionKey !== this.sessionKey &&
          current?.status !== 'active'
        ) {
          void this.selectSession(preferred.sessionKey);
        }
      }
      this.recomputeAchievements();
    } finally {
      this.achievementsLoading = false;
    }
  }

  private async loadBrainEvents(): Promise<BrainEvent[]> {
    const all: BrainEvent[] = [];
    const seen = new Set<string>();
    const now = new Date();
    const monthStart = new Date(now.getFullYear(), now.getMonth(), 1).getTime();
    let before: string | undefined;
    for (let page = 0; page < 10; page += 1) {
      let batch: BrainEvent[];
      try {
        batch = await brainEventApi.list('all', before, 100);
      } catch {
        break;
      }
      if (batch.length === 0) break;
      for (const event of batch) {
        if (seen.has(event.id)) continue;
        seen.add(event.id);
        all.push(event);
      }
      const last = batch[batch.length - 1];
      if (!last) break;
      if (new Date(last.createdAt).getTime() < monthStart) break;
      if (before === last.createdAt) break;
      before = last.createdAt;
    }
    return all;
  }

  private async selectSession(key: string): Promise<void> {
    this.closeEvents?.();
    this.closeEvents = null;
    const seq = ++this.loadSeq;
    this.sessionKey = key;
    this.loading = true;
    this.error = '';
    this.timeline.reset();
    this.story = EMPTY_STORY;
    this.historyEpoch += 1;
    this.historyBefore = null;
    this.historyHasMore = false;
    this.loadingOlder = false;
    this.historyLocked = false;
    this.stickToLatest = true;
    this.manualScroll = false;
    this.pinPending = true;
    try {
      const [page, projection] = await Promise.all([
        fetchSessionEvents(key, 0, 200, { turns: 2 }),
        fetchSessionProjection(key).catch(() => null),
      ]);
      if (seq !== this.loadSeq) return;
      this.timeline.ingest(page);
      if (projection?.turn?.agenda?.length) {
        this.timeline.agenda = projection.turn.agenda;
      }
      if (projection?.highWatermark && projection.highWatermark > page.highWatermark) {
        const missing = await fetchSessionEvents(key, page.highWatermark, 200);
        if (seq !== this.loadSeq) return;
        this.timeline.ingest(missing);
      }
      this.historyBefore = page.nextBefore ?? null;
      this.historyHasMore = Boolean(page.hasMore && this.historyBefore !== null);
      const todos = await fetchSessionTodos(key);
      if (seq !== this.loadSeq) return;
      if (todos.length > 0) this.timeline.todos = todos;
      this.rebuild();
      this.loading = false;
      this.connect(key, this.timeline.highWatermark);
      await this.updateComplete;
      this.pinToLatest();
    } catch (error) {
      if (seq !== this.loadSeq) return;
      this.error = (error as Error).message || '加载会话过程失败';
      this.loading = false;
      this.pinPending = false;
      this.suppressScroll = false;
    }
    this.recomputeAchievements();
  }

  /** 左栏向上滚动时加载更早的轮次；保持阅读锚点不跳。 */
  private async loadOlder(): Promise<void> {
    if (
      this.loading ||
      this.loadingOlder ||
      !this.historyHasMore ||
      this.historyBefore === null ||
      !this.sessionKey
    ) {
      return;
    }
    const key = this.sessionKey;
    const before = this.historyBefore;
    const epoch = this.historyEpoch;
    this.stickToLatest = false;
    const feed = this.feedScroller();
    const previousHeight = feed?.scrollHeight ?? 0;
    const previousTop = feed?.scrollTop ?? 0;
    this.loadingOlder = true;
    try {
      const page = await fetchSessionEvents(key, 0, 200, { turns: 1, before });
      if (key !== this.sessionKey || epoch !== this.historyEpoch) return;
      this.timeline.ingest(page);
      const nextBefore = page.nextBefore ?? null;
      this.historyBefore = nextBefore;
      this.historyHasMore = Boolean(page.hasMore && nextBefore !== null);
      if (nextBefore === before) this.historyHasMore = false;
      this.rebuild();
      await this.updateComplete;
      if (feed && feed.isConnected) {
        feed.scrollTop = previousTop + feed.scrollHeight - previousHeight;
      }
    } catch (error) {
      if (key === this.sessionKey) {
        this.error = (error as Error).message || '加载更早记录失败';
      }
    } finally {
      if (key === this.sessionKey && epoch === this.historyEpoch) this.loadingOlder = false;
    }
  }

  /** 桌面左栏自己滚，窄屏时整个 story-body 滚；两种情况都要能触发历史加载。 */
  private feedScroller(): HTMLElement | null {
    const main = this.querySelector('.story-main') as HTMLElement | null;
    if (main && main.scrollHeight > main.clientHeight + 1) return main;
    return this.querySelector('.story-body') as HTMLElement | null;
  }

  private onFeedWheel(event: WheelEvent): void {
    if (event.deltaY < 0) {
      this.manualScroll = true;
      this.stickToLatest = false;
    }
  }

  private onFeedScroll(): void {
    if (this.suppressScroll || this.pinPending) return;
    const feed = this.feedScroller();
    if (!feed) return;
    const distanceToBottom = feed.scrollHeight - feed.scrollTop - feed.clientHeight;
    if (this.manualScroll) {
      // A wheel-up gesture must not be cancelled by the scroll event that it
      // just produced. Only an explicit return to the very bottom resumes
      // live-follow mode.
      if (distanceToBottom <= 4) {
        this.manualScroll = false;
        this.stickToLatest = true;
      } else {
        this.stickToLatest = false;
      }
    } else {
      this.stickToLatest = distanceToBottom < 96;
    }
    // 一次上滑只放一轮：先离开顶部再回来，才允许继续自动加载。
    if (feed.scrollTop > 140) {
      this.historyLocked = false;
      return;
    }
    if (feed.scrollTop > 72 || this.historyLocked) return;
    if (this.historyHasMore && !this.loadingOlder) {
      this.historyLocked = true;
      void this.loadOlder();
    }
  }

  private loadOlderFromButton(): void {
    this.historyLocked = true;
    void this.loadOlder();
  }

  private connect(key: string, after: number): void {
    this.closeEvents = connectSessionEvents({
      sessionKey: key,
      after,
      onEvent: (event) => {
        if (this.sessionKey !== key) return;
        if (event.eventType === 'turn/start') {
          this.startStreamingTurn(key, event);
          return;
        }
        this.timeline.ingestEvent(event);
        this.rebuild();
      },
      onLive: (frame) => {
        if (this.sessionKey !== key) return;
        this.timeline.applyLive(frame);
        this.rebuild();
      },
      onError: (error) => {
        if (this.sessionKey !== key) return;
        this.error = error.message;
      },
    });
  }

  /**
   * 新一轮开始：立即把左栏切成这一轮并开始吃 SSE，保证思考、正文、
   * 工具调用能实时流出来；旧轮次不再堆在这里，向上滚动再一轮一轮取。
   */
  private startStreamingTurn(key: string, trigger: SessionEvent): void {
    if (key !== this.sessionKey) return;
    const turnId =
      typeof trigger.payloadJson?.turnId === 'string' ? trigger.payloadJson.turnId : '';
    const alreadyLoaded =
      turnId.length > 0 &&
      this.timeline.blocks.some((block) => block.kind === 'turn' && block.id === turnId);
    if (alreadyLoaded) {
      this.timeline.ingestEvent(trigger);
      this.rebuild();
      return;
    }
    this.historyEpoch += 1;
    this.timeline.reset();
    this.timeline.ingestEvent(trigger);
    this.historyBefore = trigger.seq > 0 ? trigger.seq : null;
    this.historyHasMore = this.historyBefore !== null;
    this.historyLocked = false;
    this.loadingOlder = false;
    this.stickToLatest = true;
    this.manualScroll = false;
    this.rebuild();
  }

  private rebuild(): void {
    this.story = buildStory(
      this.timeline.blocks,
      this.timeline.todos,
      this.timeline.pendingQuestion,
    );
    this.recomputeAchievements();
  }

  private recomputeAchievements(): void {
    const latest =
      this.story.sections.find((section) => section.id === this.story.latestSectionId) ??
      this.story.sections[this.story.sections.length - 1];
    const latestMs = latest ? new Date(latest.startedAt).getTime() : Number.NaN;
    this.achievements = computeAchievements({
      story: this.story,
      events: this.brainEvents,
      summary: this.brainSummary,
      storyAt: Number.isFinite(latestMs) ? latestMs : this.currentSession()?.updatedAt,
    });
  }

  private currentSession(): SessionSummary | undefined {
    return this.sessions.find((session) => session.sessionKey === this.sessionKey);
  }

  private sessionOptionLabel(session: SessionSummary): string {
    const time = formatSessionTime(session.updatedAt);
    return session.label
      ? `${session.label} · ${time}`
      : `${KIND_LABEL[session.kind] ?? '会话'} · ${time}`;
  }

  private originLabel(): string {
    const origin = this.story.origin;
    if (!origin) return '手动发起';
    return STORY_ORIGIN_LABEL[origin] ?? origin;
  }

  private headline(): string {
    const session = this.currentSession();
    if (session?.label) return session.label;
    const prompt = this.story.prompts[this.story.prompts.length - 1]?.text;
    if (prompt) return clampText(prompt, 30);
    const kind = KIND_LABEL[session?.kind ?? ''] ?? '本班';
    const latest = this.story.sections[this.story.sections.length - 1];
    return latest ? `${formatStoryClock(latest.startedAt)} 的${kind}` : `${kind}纪实`;
  }

  private isWorking(): boolean {
    if (this.timeline.pendingQuestion || this.timeline.pendingApprovals.length > 0) {
      return false;
    }
    if (
      this.story.running !== undefined ||
      this.story.sections.some((section) => section.status === 'running')
    ) {
      return true;
    }
    return false;
  }

  private statusText(): string {
    if (this.isWorking()) return ASSISTANT_STATUS.working;
    if (this.timeline.pendingQuestion || this.timeline.pendingApprovals.length > 0) {
      return ASSISTANT_STATUS.waiting_for_user;
    }
    return ASSISTANT_STATUS.idle;
  }

  private pendingCount(): number {
    return (this.timeline.pendingQuestion ? 1 : 0) + this.timeline.pendingApprovals.length;
  }

  private openEngineer(event: Event): void {
    event.preventDefault();
    if (!this.sessionKey) return;
    window.history.pushState(
      {},
      '',
      `/chat#/assistant/sessions/${encodeURIComponent(this.sessionKey)}`,
    );
    window.dispatchEvent(new PopStateEvent('popstate'));
  }

  /** Pin after a load or while new content is appended. */
  private pinFeedToBottom(): void {
    const pin = () => {
      const feed = this.feedScroller();
      if (feed) feed.scrollTop = feed.scrollHeight;
    };
    pin();
    requestAnimationFrame(pin);
  }

  /** One-shot initial pin: ignore the scroll events our own scroll causes. */
  private pinToLatest(): void {
    this.suppressScroll = true;
    const pin = () => {
      const feed = this.feedScroller();
      if (feed) feed.scrollTop = feed.scrollHeight;
    };
    pin();
    requestAnimationFrame(() => {
      pin();
      requestAnimationFrame(() => {
        pin();
        this.suppressScroll = false;
        this.pinPending = false;
      });
    });
  }

  protected updated(): void {
    if (!this.stickToLatest || this.loadingOlder || this.suppressScroll) return;
    this.pinFeedToBottom();
  }

  render() {
    const session = this.currentSession();
    return html`
      <div class="story-shell">
        <header class="story-topbar">
          <div class="story-topbar__identity">
            <span class="story-topbar__mark" aria-hidden="true">脑</span>
            <div>
              <div class="story-topbar__title">AI 工作纪实</div>
              <div class="story-topbar__sub">看得懂的业务视角</div>
            </div>
          </div>
          <span class="story-topbar__spacer"></span>
          <label class="story-select">
            <span class="story-select__label">切换记录</span>
            <select
              class="story-select__input"
              .value=${this.sessionKey}
              @change=${(event: Event) => {
                this.manualSelection = true;
                void this.selectSession((event.target as HTMLSelectElement).value);
              }}
            >
              ${this.sessions
                .slice(0, 20)
                .map(
                  (item) =>
                    html`<option value=${item.sessionKey}>
                      ${this.sessionOptionLabel(item)}
                    </option>`,
                )}
              ${
                session === undefined && this.sessionKey
                  ? html`<option value=${this.sessionKey}>当前记录</option>`
                  : nothing
              }
            </select>
          </label>
          <button class="story-topbar__link" @click=${(event: Event) => this.openEngineer(event)}>
            工程师视图 →
          </button>
        </header>

        <div class="story-body" @scroll=${() => this.onFeedScroll()} @wheel=${(event: WheelEvent) => this.onFeedWheel(event)}>
          <div class="story-work">
          <div class="story-head">
            ${this.renderAgenda()}
            ${this.renderHero()}
            ${this.renderFailureNotice()} ${this.renderWaiting()}
          </div>
          <main class="story-main" @scroll=${() => this.onFeedScroll()}>
            <div class="story-feed">
              ${this.renderHistory()}
              ${this.renderPrompt()}
              ${
                this.loading && this.story.sections.length === 0
                  ? this.renderSkeleton()
                  : this.story.sections.length === 0
                    ? this.renderEmpty()
                    : this.story.sections.map((section, index) =>
                        this.renderSection(section, index),
                      )
              }
              ${this.renderLive()}
              ${this.error ? html`<div class="story-error">${this.error}</div>` : nothing}
            </div>
          </main>
          ${
            this.stickToLatest && this.story.plans.some((todo) => todo.status !== 'completed')
              ? html`<div class="story-task-dock">
                  <task-plan-panel
                    .todos=${this.story.plans}
                    ?running=${this.isWorking()}
                    ?startCollapsed=${true}
                  ></task-plan-panel>
                </div>`
              : nothing
          }
          </div>
          <aside class="story-rail" aria-label="工作成果">
            <story-achievements
              .stats=${this.achievements}
              ?loading=${this.achievementsLoading}
            ></story-achievements>
            <story-device-deck
              .devices=${this.story.latestDevices}
              ?working=${this.isWorking()}
            ></story-device-deck>
            <story-vision-deck
              .visions=${this.story.latestVision}
              .workspaceId=${this.workspaceId}
              ?working=${this.isWorking()}
            ></story-vision-deck>
          </aside>
        </div>
      </div>
    `;
  }

  private renderHistory() {
    if (this.loading && this.story.sections.length === 0) return nothing;
    if (!this.historyHasMore && this.story.sections.length <= 2) return nothing;
    return html`
      <div class="story-history">
        ${
          this.loadingOlder
            ? html`<span class="story-history__loading">
                <span class="story-history__spinner" aria-hidden="true"></span>
                正在加载更早记录…
              </span>`
            : this.historyHasMore
              ? html`<button class="story-history__button" @click=${() => this.loadOlderFromButton()}>
                    ↑ 加载更早记录
                  </button>
                  <span class="story-history__hint">向上滚动自动加载 · 每次 1 轮</span>`
              : html`<span class="story-history__end">已到最早记录</span>`
        }
      </div>
    `;
  }

  private renderHero() {
    const stats = this.story.latestStats;
    const needs = this.pendingCount();
    return html`
      <section class="story-hero" aria-label="最近一轮概览">
        <div class="story-hero__eyebrow">
          <span
            class="story-hero__dot ${this.isWorking() ? 'is-live' : ''}"
            aria-hidden="true"
          ></span>
          最近一轮 · ${this.originLabel()}
          <span class="story-hero__loaded">已载入 ${this.story.sections.length} 轮</span>
          <span class="story-hero__status">当前${this.statusText()}</span>
        </div>
        <h1 class="story-hero__title">${this.headline()}</h1>
        <p class="story-hero__lead">
          ${this.originLabel() === '手动发起' ? '最近一轮工作' : `由${this.originLabel()}发起的最近一轮`}，
          ${stats.devices > 0 ? `查看了 ${stats.devices} 台设备` : '还没有查看具体设备'}${
            stats.actions > 0 ? `，执行了 ${stats.actions} 次设备处置` : ''
          }。
        </p>
        <div class="story-hero__stats">
          <div class="story-hero__stat"><strong>${stats.devices}</strong><span>台设备</span></div>
          <div class="story-hero__stat"><strong>${stats.vision}</strong><span>张画面</span></div>
          <div class="story-hero__stat ${stats.findings > 0 ? 'is-alert' : ''}">
            <strong>${stats.findings}</strong><span>项发现</span>
          </div>
          <div class="story-hero__stat"><strong>${stats.actions}</strong><span>次处置</span></div>
          ${
            needs > 0
              ? html`<a class="story-hero__needs" href="/dispositions">${needs} 件等你确认</a>`
              : nothing
          }
        </div>
      </section>
    `;
  }

  private renderAgenda() {
    const agenda = this.timeline.agenda;
    if (agenda.length === 0) return nothing;
    return html`<div class="story-agenda" aria-label="本轮技能">
      <span class="story-agenda__label">本轮技能</span>
      ${agenda.map(
        (item) => html`<span class="story-agenda__item" data-status=${item.status}>
          <span class="story-agenda__dot" aria-hidden="true"></span>${item.label}
        </span>`,
      )}
    </div>`;
  }

  private renderWaiting() {
    const approvals = this.timeline.pendingApprovals;
    if (approvals.length > 0) {
      return html`<div class="story-waiting story-waiting--approval" role="status">
        <div class="story-waiting__head">
          <span class="story-waiting__mark" aria-hidden="true">!</span>
          <div>
            <strong>等待你批准</strong>
            <p>可以直接在这里处理，批准后 AI 会继续执行。</p>
          </div>
        </div>
        <div class="story-waiting__approvals">
          ${approvals.map(
            (approval) => html`<approval-card
              .sessionKey=${this.sessionKey}
              .pending=${approval}
              @answered=${(event: CustomEvent<string>) => this.onApprovalAnswered(event)}
            ></approval-card>`,
          )}
        </div>
        <button class="story-waiting__action" @click=${(event: Event) => this.openEngineer(event)}>
          查看完整上下文
        </button>
      </div>`;
    }
    const question = this.timeline.pendingQuestion;
    if (!question) return nothing;
    return html`<div class="story-waiting story-waiting--question" role="status">
      <question-card
        .sessionKey=${this.sessionKey}
        .pending=${question}
        @answered=${() => this.onQuestionAnswered()}
      ></question-card>
      <button class="story-waiting__action" @click=${(event: Event) => this.openEngineer(event)}>
        查看完整上下文
      </button>
    </div>`;
  }

  private onQuestionAnswered(): void {
    this.timeline.pendingQuestion = null;
    this.rebuild();
    void this.refreshAfterApproval();
  }

  private onApprovalAnswered(event: CustomEvent<string>): void {
    this.timeline.pendingApprovals = this.timeline.pendingApprovals.filter(
      (approval) => approval.approvalId !== event.detail,
    );
    this.rebuild();
    void this.refreshAfterApproval();
  }

  private async refreshAfterApproval(): Promise<void> {
    const key = this.sessionKey;
    if (!key) return;
    try {
      const page = await fetchSessionEvents(key, this.timeline.highWatermark, 200);
      if (key !== this.sessionKey) return;
      this.timeline.ingest(page);
      this.rebuild();
    } catch {
      // SSE will deliver the approval/resolved and turn continuation.
    }
  }

  private renderFailureNotice() {
    const latest = this.story.sections[this.story.sections.length - 1];
    if (!latest || (latest.status !== 'failed' && latest.status !== 'cancelled')) {
      return nothing;
    }
    const copy = failureCopy(latest.error ?? latest.lastRetryError ?? '');
    const title = latest.status === 'failed' ? '本轮没有完成' : '本轮已取消';
    const retry = latest.retries > 0 ? ` 已重试 ${latest.retries} 次。` : '';
    return html`<div class="story-failure" role="alert">
      <span class="story-failure__mark" aria-hidden="true">!</span>
      <div class="story-failure__body">
        <strong>${title}</strong>
        <p>${copy.detail}${retry}</p>
      </div>
    </div>`;
  }

  private renderPrompt() {
    const prompt = this.story.prompts[this.story.prompts.length - 1];
    if (!prompt) return nothing;
    return html`
      <div class="story-prompt">
        <span class="story-prompt__who">你</span>
        <div class="story-prompt__body">
          <p>${prompt.text}</p>
          <time>${formatStoryClock(prompt.at)}</time>
        </div>
      </div>
    `;
  }

  private renderSection(section: StorySection, index: number) {
    const isLast = index === this.story.sections.length - 1;
    const isLatest = section.id === this.story.latestSectionId;
    const origin = STORY_ORIGIN_LABEL[section.origin ?? ''] ?? '手动发起';
    return html`
      <article
        class="story-chapter story-chapter--${section.status} ${
          isLatest ? 'story-chapter--latest' : ''
        }"
      >
        <div class="story-chapter__rail" aria-hidden="true">
          <span class="story-chapter__dot"></span>
        </div>
        <header class="story-chapter__head">
          <time class="story-chapter__time">${formatStoryClock(section.startedAt)}</time>
          <span class="story-chapter__origin">${origin}</span>
          ${
            isLatest
              ? html`<span class="story-chapter__latest">${isLast ? '本轮' : '最近动作'}</span>`
              : nothing
          }
          <span class="story-chapter__status"
            >${SECTION_STATUS[section.status] ?? section.status}</span
          >
          ${
            section.findingCount > 0
              ? html`<span class="story-chapter__findings">发现 ${section.findingCount} 项</span>`
              : nothing
          }
        </header>
        ${isLast ? this.renderPlan() : nothing}
        ${
          section.steps.length > 0
            ? html`<div class="story-steps">
                ${section.steps.map((step) => this.renderStep(step))}
              </div>`
            : nothing
        }
        ${
          section.conclusion
            ? html`<div class="story-conclusion">
                <div class="story-conclusion__head">
                  <span class="story-conclusion__avatar" aria-hidden="true">脑</span>
                  AI 的结论
                </div>
                <div class="story-conclusion__body">${unsafeHTML(md(section.conclusion))}</div>
              </div>`
            : nothing
        }
      </article>
    `;
  }

  private renderPlan() {
    const todos: TodoItem[] = this.story.plans;
    if (todos.length === 0) return nothing;
    const done = todos.filter((todo) => todo.status === 'completed').length;
    const percent = Math.round((done / todos.length) * 100);
    return html`
      <details class="story-plan">
        <summary class="story-plan__head">
          <span>本轮计划</span>
          <span class="story-plan__count">${done}/${todos.length}</span>
        </summary>
        <div class="story-plan__meter" aria-hidden="true">
          <span style=${`width:${percent}%`}></span>
        </div>
        <ol class="story-plan__list">
          ${todos.map(
            (todo) =>
              html`<li class="story-plan__item story-plan__item--${todo.status}">
                <span class="story-plan__mark" aria-hidden="true">
                  ${todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '▸' : '·'}
                </span>
                <span>${todo.content}</span>
              </li>`,
          )}
        </ol>
      </details>
    `;
  }

  private renderStep(step: StoryStep) {
    return html`
      <details class="story-step story-step--${step.tone} story-step--${step.status}">
        <summary class="story-step__line">
          <span class="story-step__mark" aria-hidden="true">
            <span class="story-step__icon">${STEP_ICON[step.tone]}</span>
          </span>
          <span class="story-step__title">
            <span class="story-step__title-closed">${step.title}</span>
            <span class="story-step__title-open">${step.title} · 详情</span>
          </span>
          ${
            step.detail
              ? html`<span class="story-step__brief">${clampText(step.detail, 72)}</span>`
              : nothing
          }
          ${step.finding ? html`<span class="story-step__finding">${step.finding}</span>` : nothing}
          <time class="story-step__time">${formatStoryClock(step.at)}</time>
          <span class="story-step__chevron" aria-hidden="true">
            <svg viewBox="0 0 24 24"><path d="m6 9 6 6 6-6"/></svg>
          </span>
        </summary>
        <div class="story-step__expand">
          ${step.detail ? html`<p class="story-step__detail">${step.detail}</p>` : nothing}
          ${
            step.metrics && step.metrics.length > 0
              ? html`<dl class="story-step__metrics">
                  ${step.metrics.map(
                    (metric) => html`<div class="story-step__metric">
                      <dt>${metric.name}</dt>
                      <dd>${metric.value}</dd>
                    </div>`,
                  )}
                </dl>`
              : nothing
          }
          ${
            step.details && step.details.length > 0
              ? html`<dl class="story-step__details">
                  ${step.details.map(
                    (detail) => html`<div class="story-step__detail-row">
                      <dt>${detail.label}</dt>
                      <dd>${detail.value}</dd>
                    </div>`,
                  )}
                </dl>`
              : nothing
          }
          <div class="story-step__meta">
            <time>${formatStoryClock(step.at)}</time>
            <span>${STEP_STATUS[step.status]}</span>
            ${step.tool ? html`<span class="story-step__tool">${step.tool}</span>` : nothing}
            ${
              step.visionIds.length > 0
                ? html`<span>${step.visionIds.length} 张现场画面</span>`
                : nothing
            }
          </div>
        </div>
      </details>
    `;
  }

  private renderLive() {
    const running = this.story.running;
    if (!running) return nothing;
    return html`<div class="story-live">
      <span class="story-live__dot" aria-hidden="true"></span>
      <span>正在${running.title}</span>
    </div>`;
  }

  private renderEmpty() {
    return html`<div class="story-empty">
      <div class="story-empty__title">这班岗还没有记录</div>
      <p>AI 还没有开始工作，或这次会话没有留下可展示的过程。</p>
      <a class="story-empty__link" href="/chat">去 AI 助手发起一次工作</a>
    </div>`;
  }

  private renderSkeleton() {
    return html`<div class="story-skeleton" aria-label="正在读取工作记录">
      ${[0, 1, 2].map(
        () =>
          html`<div class="story-skeleton__row">
            <span class="story-skeleton__mark"></span>
            <span class="story-skeleton__line"></span>
          </div>`,
      )}
    </div>`;
  }


}

declare global {
  interface HTMLElementTagNameMap {
    'view-story': StoryView;
  }
}
