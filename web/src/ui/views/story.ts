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
  fetchAssistantHud,
  fetchSessionEvents,
  fetchSessionTodos,
  listChatSessions,
  type HudSnapshot,
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
};

const STEP_STATUS: Record<StoryStep['status'], string> = {
  running: '进行中',
  done: '完成',
  failed: '失败',
};

const STEP_ICON: Record<StoryStep['tone'], string> = {
  read: '◎',
  write: '✎',
  alarm: '⚠',
  plan: '☑',
  ask: '?',
  analysis: '◌',
  skill: '◆',
  think: '✧',
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

/** 默认记录：优先正在跑的，其次最近的自主巡检/事件，最后任意会话。 */
export function pickDefaultSession(sessions: SessionSummary[]): SessionSummary | null {
  if (sessions.length === 0) return null;
  const rank = (session: SessionSummary): number =>
    (session.status === 'active' ? 2 : 0) +
    (session.kind === 'scheduled' || session.kind === 'event' ? 1 : 0);
  return [...sessions].sort(
    (left, right) => rank(right) - rank(left) || right.updatedAt - left.updatedAt,
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
  @state() private hud: HudSnapshot | null = null;
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
  private stickToLatest = true;
  private historyEpoch = 0;

  connectedCallback(): void {
    super.connectedCallback();
    void this.bootstrap();
  }

  disconnectedCallback(): void {
    this.closeEvents?.();
    this.closeEvents = null;
    if (this.refreshTimer !== null) window.clearInterval(this.refreshTimer);
    this.refreshTimer = null;
    super.disconnectedCallback();
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
      const [summary, events, hud, sessions] = await Promise.all([
        brainEventApi.summary().catch(() => null),
        this.loadBrainEvents(),
        fetchAssistantHud(this.workspaceId).catch(() => null),
        listChatSessions().catch(() => null),
      ]);
      this.brainSummary = summary;
      this.brainEvents = events;
      this.hud = hud;
      if (sessions) {
        const previous = this.sessions;
        this.sessions = sessions;
        const preferred = pickDefaultSession(sessions);
        const current = previous.find((session) => session.sessionKey === this.sessionKey);
        if (
          !this.manualSelection &&
          preferred &&
          preferred.sessionKey !== this.sessionKey &&
          current?.status !== 'active' &&
          preferred.status === 'active'
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
    try {
      const page = await fetchSessionEvents(key, 0, 200, { turns: 2 });
      if (seq !== this.loadSeq) return;
      this.timeline.ingest(page);
      this.historyBefore = page.nextBefore ?? null;
      this.historyHasMore = Boolean(page.hasMore && this.historyBefore !== null);
      const todos = await fetchSessionTodos(key);
      if (seq !== this.loadSeq) return;
      if (todos.length > 0) this.timeline.todos = todos;
      this.rebuild();
      this.loading = false;
      this.connect(key, this.timeline.highWatermark);
    } catch (error) {
      if (seq !== this.loadSeq) return;
      this.error = (error as Error).message || '加载会话过程失败';
      this.loading = false;
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

  private onFeedScroll(): void {
    const feed = this.feedScroller();
    if (!feed) return;
    const distanceToBottom = feed.scrollHeight - feed.scrollTop - feed.clientHeight;
    this.stickToLatest = distanceToBottom < 96;
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
        if (event.eventType === 'turn.started') {
          this.startStreamingTurn(key, event);
          return;
        }
        this.timeline.ingestEvent(event);
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
    this.rebuild();
  }

  private rebuild(): void {
    this.story = buildStory(this.timeline.blocks, this.timeline.todos);
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
    if (
      this.story.running !== undefined ||
      this.story.sections.some((section) => section.status === 'running')
    ) {
      return true;
    }
    // 还没有任何 turn 事件时，才用 HUD 兜底，避免 HUD 的 60s 缓存把“已完成”
    // 误显示成“正在工作”。
    return this.story.sections.length === 0 && this.hud?.assistant.status === 'working';
  }

  private workingDetail(): string {
    if (this.story.running) return this.story.running.title;
    if (this.story.sections.some((section) => section.status === 'running')) {
      return '正在思考下一步';
    }
    return '正在处理新的任务';
  }

  private statusText(): string {
    if (this.isWorking()) return ASSISTANT_STATUS.working;
    const hudStatus = this.hud?.assistant.status;
    if (hudStatus === 'waiting_for_user' || hudStatus === 'paused' || hudStatus === 'error') {
      return ASSISTANT_STATUS[hudStatus] ?? hudStatus;
    }
    return ASSISTANT_STATUS.idle;
  }

  private pendingCount(): number {
    if (!this.hud) return 0;
    return this.hud.pending.approvals + this.hud.pending.questions;
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

  protected updated(): void {
    if (!this.stickToLatest || this.loadingOlder) return;
    const pin = () => {
      const feed = this.feedScroller();
      if (feed) feed.scrollTop = feed.scrollHeight;
    };
    pin();
    requestAnimationFrame(pin);
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
          ${
            this.isWorking()
              ? html`<span class="story-topbar__working">
                  <span class="story-topbar__working-dot" aria-hidden="true"></span>
                  大脑工作中
                </span>`
              : nothing
          }
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

        <div class="story-body" @scroll=${() => this.onFeedScroll()}>
          <main class="story-main" @scroll=${() => this.onFeedScroll()}>
            <div class="story-feed">
              ${this.renderHistory()} ${this.renderHero()}
              ${this.isWorking() ? this.renderWorking() : nothing} ${this.renderPrompt()}
              ${
                this.loading && this.story.sections.length === 0
                  ? this.renderSkeleton()
                  : this.story.sections.length === 0
                    ? this.renderEmpty()
                    : this.story.sections.map((section, index) =>
                        this.renderSection(section, index),
                      )
              }
              ${this.renderFindings()} ${this.renderLive()}
              ${this.error ? html`<div class="story-error">${this.error}</div>` : nothing}
              ${this.renderFooter()}
            </div>
          </main>
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

  private renderWorking() {
    return html`
      <div class="story-working" role="status" aria-live="polite">
        <span class="story-working__orb" aria-hidden="true"><span></span></span>
        <div class="story-working__text">
          <strong>AI 大脑正在工作</strong>
          <span>${this.workingDetail()}</span>
        </div>
        <span class="story-working__bars" aria-hidden="true"> <i></i><i></i><i></i><i></i> </span>
      </div>
    `;
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
      <details class="story-plan" open>
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
          <span class="story-step__mark" aria-hidden="true">${STEP_ICON[step.tone]}</span>
          <span class="story-step__title">${step.title}</span>
          ${
            step.detail
              ? html`<span class="story-step__brief">${clampText(step.detail, 72)}</span>`
              : nothing
          }
          ${step.finding ? html`<span class="story-step__finding">${step.finding}</span>` : nothing}
          <time class="story-step__time">${formatStoryClock(step.at)}</time>
          <span class="story-step__chevron" aria-hidden="true">⌄</span>
        </summary>
        <div class="story-step__expand">
          ${step.detail ? html`<p class="story-step__detail">${step.detail}</p>` : nothing}
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

  private renderFindings() {
    if (this.story.sections.length === 0 || this.story.latestStats.devices === 0) return nothing;
    if (this.story.latestFindings.length === 0) {
      if (this.isWorking()) return nothing;
      return html`<section class="story-findings story-findings--clear" aria-label="本轮发现">
        <div class="story-findings__head">
          <span class="story-findings__eyebrow">本轮发现</span>
          <span class="story-findings__count">0 项</span>
        </div>
        <p class="story-findings__clear">最近一轮没有发现异常，设备状态正常。</p>
      </section>`;
    }
    return html`<section class="story-findings" aria-label="本轮发现">
      <div class="story-findings__head">
        <span class="story-findings__eyebrow">本轮发现</span>
        <span class="story-findings__count">${this.story.latestFindings.length} 项</span>
      </div>
      <ul class="story-findings__list">
        ${this.story.latestFindings.map(
          (finding) =>
            html`<li class="story-findings__item">
              <span class="story-findings__device">${finding.deviceName}</span>
              <span class="story-findings__label">${finding.label}</span>
              <time>${formatStoryClock(finding.at)}</time>
            </li>`,
        )}
      </ul>
    </section>`;
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

  private renderFooter() {
    if (!this.sessionKey) return nothing;
    return html`<footer class="story-foot">
      <span>这是 AI 的工作纪实，原始工具轨迹可在工程师视图中查看。</span>
      <button class="story-foot__link" @click=${(event: Event) => this.openEngineer(event)}>
        查看完整轨迹 →
      </button>
    </footer>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-story': StoryView;
  }
}
