/**
 * AI 工作纪实 · 星云流水（新页面，story 页保持不动）。
 *
 * 数据完全复用 story 的链路：listChatSessions → fetchSessionEvents →
 * TimelineModel → buildStory → buildScene。这里只负责渲染：左侧语音球、
 * 中间技能行 + 卡片连线、顶部待确认、底部本轮总结。
 */

import { LitElement, html, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import {
  connectSessionEvents,
  fetchSessionEvents,
  fetchSessionProjection,
  listChatSessions,
  type SessionSummary,
} from '../../api/assistant.js';
import { getWorkspaceId } from '../../api/client.js';
import { TimelineModel } from '../chat/timeline-model.js';
import {
  buildStory,
  EMPTY_STORY,
  STORY_ORIGIN_LABEL,
  type StoryView,
} from '../story/story-model.js';
import {
  buildScene,
  type SceneCard,
  type SceneLane,
  type SceneNote,
  type ScenePending,
  type StoryScene,
} from '../story/story-scene.js';
import '../chat/approval-card.js';
import '../chat/question-card.js';
import '../story/evidence-image.js';
import { MapBeams } from '../story/story-map-beams.js';
import { MAP_COPY } from '../story/story-map-copy.js';
import { MapOrb } from '../story/story-map-orb.js';
import { formatClock, pickDefaultSession, sessionLabel } from '../story/story-session.js';
import '../../styles/views/story-map.css';

const SESSION_KEY = 'tinyiothub_story_map_session';



@customElement('view-story-map')
export class StoryMapView extends LitElement {
  createRenderRoot() {
    return this;
  }

  @state() private sessions: SessionSummary[] = [];
  @state() private sessionKey = '';
  @state() private story: StoryView = EMPTY_STORY;
  @state() private scene: StoryScene = buildScene(EMPTY_STORY);
  @state() private loading = true;
  @state() private error = '';
  @state() private live = false;
  @state() private sectionId = '';
  @state() private historyBefore: number | null = null;
  @state() private historyHasMore = false;
  @state() private loadingOlder = false;
  @state() private answerOpen = false;
  @state() private hasNew = false;
  @state() private openNotes = new Set<string>();
  @state() private noteFocus: string | null = null;
  @state() private nowScroll = { overflow: false, thumb: 1, top: 0 };
  @state() private tip: { x: number; y: number; text: string; width: number } | null = null;

  private timeline = new TimelineModel();
  private closeEvents: (() => void) | null = null;
  private refreshTimer: number | null = null;
  private loadSeq = 0;
  private orb: MapOrb | null = null;
  private beams: MapBeams | null = null;
  private observer: MutationObserver | null = null;

  connectedCallback(): void {
    super.connectedCallback();
    void this.bootstrap();
  }

  disconnectedCallback(): void {
    this.closeEvents?.();
    this.closeEvents = null;
    if (this.refreshTimer !== null) window.clearInterval(this.refreshTimer);
    this.refreshTimer = null;
    this.orb?.dispose();
    this.orb = null;
    this.beams?.dispose();
    this.beams = null;
    this.observer?.disconnect();
    this.observer = null;
    window.removeEventListener('resize', this.onResize);
    super.disconnectedCallback();
  }

  protected firstUpdated(): void {
    const canvas = this.querySelector<HTMLCanvasElement>('.map-canvas');
    const svg = this.querySelector<SVGSVGElement>('.map-beams');
    const shell = this.querySelector<HTMLElement>('.map-shell');
    if (canvas) this.orb = new MapOrb(canvas);
    if (svg && shell) this.beams = new MapBeams(shell, svg);
    this.observer = new MutationObserver(() => this.beams?.schedule());
    this.observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    window.addEventListener('resize', this.onResize);
    // 行内横向滚动、纵向滚动都发生在内部容器里，用捕获阶段统一监听。
    this.addEventListener('scroll', this.onScroll, { passive: true, capture: true });
    this.beams?.schedule();
  }

  private onResize = (): void => {
    this.beams?.schedule();
  };

  private onScroll = (): void => {
    this.beams?.schedule();
  };

  private get workspaceId(): string {
    return getWorkspaceId() ?? '';
  }

  private get currentSession(): SessionSummary | undefined {
    return this.sessions.find((session) => session.sessionKey === this.sessionKey);
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
      const stored = localStorage.getItem(SESSION_KEY);
      const preferred = stored ? sessions.find((item) => item.sessionKey === stored) : undefined;
      const selected = preferred ?? pickDefaultSession(sessions);
      if (selected) {
        await this.selectSession(selected.sessionKey);
      } else {
        this.loading = false;
      }
    } catch (error) {
      this.error = (error as Error).message || '加载工作记录失败';
      this.loading = false;
    }
    if (this.refreshTimer === null) {
      this.refreshTimer = window.setInterval(() => void this.refreshSessions(), 60_000);
    }
  }

  private async refreshSessions(): Promise<void> {
    try {
      const sessions = await listChatSessions();
      this.sessions = sessions;
    } catch {
      /* 列表刷新失败不打断当前页面 */
    }
  }

  private async selectSession(key: string): Promise<void> {
    this.closeEvents?.();
    this.closeEvents = null;
    const seq = ++this.loadSeq;
    this.sessionKey = key;
    localStorage.setItem(SESSION_KEY, key);
    this.loading = true;
    this.error = '';
    this.live = false;
    this.timeline.reset();
    this.sectionId = '';
    this.historyBefore = null;
    this.historyHasMore = false;
    this.loadingOlder = false;
    this.answerOpen = false;
    this.hasNew = false;
    this.story = EMPTY_STORY;
    this.scene = buildScene(EMPTY_STORY);
    try {
      const [page, projection] = await Promise.all([
        fetchSessionEvents(key, 0, 200, { turns: 2 }),
        fetchSessionProjection(key).catch(() => null),
      ]);
      if (seq !== this.loadSeq) return;
      this.timeline.ingest(page);
      this.historyBefore = page.nextBefore ?? null;
      this.historyHasMore = Boolean(page.hasMore && this.historyBefore !== null);
      if (projection?.turn?.agenda?.length) this.timeline.agenda = projection.turn.agenda;
      if (projection?.turn?.todos) this.timeline.todos = projection.turn.todos;
      if (projection?.highWatermark && projection.highWatermark > page.highWatermark) {
        const missing = await fetchSessionEvents(key, page.highWatermark, 200);
        if (seq !== this.loadSeq) return;
        this.timeline.ingest(missing);
      }
      this.rebuild();
      this.loading = false;
      this.connect(key);
      await this.updateComplete;
      this.beams?.schedule();
    } catch (error) {
      if (seq !== this.loadSeq) return;
      this.error = (error as Error).message || '加载会话过程失败';
      this.loading = false;
    }
  }

  private connect(key: string): void {
    this.closeEvents = connectSessionEvents({
      sessionKey: key,
      after: this.timeline.highWatermark,
      onEvent: (event) => {
        if (this.sessionKey !== key) return;
        try {
          this.timeline.ingestEvent(event);
          this.rebuild();
          this.flagNewContent();
        } catch (error) {
          console.error('story-map ingest failed', error);
        }
      },
      onLive: (frame) => {
        if (this.sessionKey !== key) return;
        this.live = true;
        try {
          this.timeline.applyLive(frame);
          this.rebuild();
          this.flagNewContent();
        } catch (error) {
          console.error('story-map live frame failed', error);
        }
      },
      onError: () => {
        if (this.sessionKey === key) this.live = false;
      },
    });
  }

  private rebuild(): void {
    try {
      this.story = buildStory(
        this.timeline.blocks,
        this.timeline.todos,
        this.timeline.pendingQuestion,
      );
      this.scene = buildScene(this.story, {
        question: this.timeline.pendingQuestion,
        approvals: this.timeline.pendingApprovals,
        plans: this.timeline.todos,
        sectionId: this.sectionId || undefined,
      });
      queueMicrotask(() => {
        this.beams?.schedule();
        this.orb?.ping();
      });
    } catch (error) {
      console.error('story-map rebuild failed', error);
      this.error = '这一轮记录暂时无法展示，请切换记录或打开工程师视图。';
    }
  }

  private showTip(event: Event, text: string): void {
    const el = event.currentTarget as HTMLElement;
    if (!text || el.scrollWidth <= el.clientWidth + 1) return;
    const rect = el.getBoundingClientRect();
    const width = Math.min(360, window.innerWidth - 24);
    const x = Math.min(Math.max(rect.left, 12), window.innerWidth - width - 12);
    this.tip = { x, y: rect.bottom + 6, text, width };
  }

  private hideTip(): void {
    this.tip = null;
  }

  private toggleTip(event: Event, text: string): void {
    if (this.tip) {
      this.tip = null;
      return;
    }
    this.showTip(event, text);
  }

  private switchSection(event: Event): void {
    this.sectionId = (event.target as HTMLSelectElement).value;
    this.scene = buildScene(this.story, {
      question: this.timeline.pendingQuestion,
      approvals: this.timeline.pendingApprovals,
      plans: this.timeline.todos,
      sectionId: this.sectionId || undefined,
    });
    void this.updateComplete.then(() => this.beams?.schedule());
  }

  private switchSession(event: Event): void {
    const key = (event.target as HTMLSelectElement).value;
    if (key && key !== this.sessionKey) void this.selectSession(key);
  }

  private openEngineer(): void {
    if (!this.sessionKey) return;
    window.history.pushState({}, '', `/chat#/assistant/sessions/${encodeURIComponent(this.sessionKey)}`);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }

  private openClassic(): void {
    window.history.pushState({}, '', '/story');
    window.dispatchEvent(new PopStateEvent('popstate'));
  }

  private originLabel(): string {
    const origin = this.story.origin;
    return (origin && STORY_ORIGIN_LABEL[origin]) || '手动发起';
  }

  /* ── 渲染 ───────────────────────────────────────────── */

  protected render() {
    const scene = this.scene;
    const section = this.story.sections[this.story.sections.length - 1];
    const running = section?.status === 'running';
    const session = this.currentSession;

    return html`
      <div class="map-shell" @click=${(event: MouseEvent) => this.onShellClick(event)}>
        <canvas class="map-canvas" aria-hidden="true"></canvas>
        <svg class="map-beams" aria-hidden="true"></svg>
        ${this.tip
          ? html`<div
              class="map-tip"
              style=${`left:${this.tip.x}px; top:${this.tip.y}px; max-width:${this.tip.width}px`}
            >
              ${this.tip.text}
            </div>`
          : nothing}

        <aside class="map-brain">
          <div class="map-brain__id">
            <div class="map-eyebrow">AI 大脑</div>
            <h1 class="map-title">${session?.label || 'AI 工作纪实'}</h1>
            <p class="map-meta">
              ${this.originLabel()} · ${section ? formatClock(section.startedAt) : '—'} 开始 ·
              ${running ? MAP_COPY.now.running : MAP_COPY.now.ended}
            </p>
          </div>

          <div class="map-now">
            <div class="map-now__card">
              <div class="map-now__label">
                <span class="map-now__dot ${running ? 'is-live' : ''}"></span>
                ${running ? MAP_COPY.now.running : MAP_COPY.now.idle}
              </div>
              <div class="map-now__title">${scene.lanes[scene.lanes.length - 1]?.skill || '等待执行'}</div>
              <div class="map-now__body">
                <p class="map-now__text" @scroll=${() => this.refreshNowScroll()}>
                  ${scene.summary.text}
                </p>
                ${this.nowScroll.overflow
                  ? html`<span class="map-now__bar" aria-hidden="true">
                      <i
                        style=${`height:${this.nowScroll.thumb * 100}%;top:${this.nowScroll.top *
                        (100 - this.nowScroll.thumb * 100)}%`}
                      ></i>
                    </span>`
                  : nothing}
              </div>
              <div class="map-now__meta">
                <span>${scene.lanes.length} 个技能</span>
                <i></i>
                <span>${scene.summary.steps} 步</span>
              </div>
            </div>
          </div>

          <div class="map-scope">
            <div class="map-scope__row">
              <div class="map-scope__item"><b>${scene.summary.devices}</b>台设备</div>
              <div class="map-scope__item"><b>${scene.summary.vision}</b>张画面</div>
              <div class="map-scope__item"><b>${scene.summary.findings}</b>项发现</div>
              <div class="map-scope__item"><b>${scene.summary.actions}</b>次处置</div>
              ${scene.summary.failedActions > 0
                ? html`<div class="map-scope__item"><b>${scene.summary.failedActions}</b>次失败</div>`
                : nothing}
            </div>
            <div class="map-lanemap">
              ${scene.lanes.map(
                (lane) => html`<span class="map-lanemap__row">
                  <span>${lane.skill}</span><span class="map-lanemap__bar"></span
                  ><span class="map-lanemap__t">${lane.at ? formatClock(lane.at) : '—'}</span>
                </span>`,
              )}
            </div>
          </div>
        </aside>

        <section class="map-lanes" @scroll=${() => this.onLanesScroll()}>
          <header class="map-toolbar">
            <label class="map-select">
              <span>${MAP_COPY.toolbar.record}</span>
              <select @change=${(event: Event) => this.switchSession(event)}>
                ${this.sessions.map(
                  (item) => html`<option value=${item.sessionKey} ?selected=${item.sessionKey === this.sessionKey}>
                    ${sessionLabel(item)}
                  </option>`,
                )}
              </select>
            </label>
            ${this.story.sections.length > 1
              ? html`<label class="map-select">
                  <span>${MAP_COPY.toolbar.turn}</span>
                  <select @change=${(event: Event) => this.switchSection(event)}>
                    ${[...this.story.sections].reverse().map((item) => {
                      const steps = item.steps.length;
                      const label = `${formatClock(item.startedAt)} · ${sectionStatusLabel(item.status)} · ${steps} 步`;
                      return html`<option value=${item.id} ?selected=${item.id === (this.sectionId || this.story.latestSectionId)}>
                        ${label}
                      </option>`;
                    })}
                  </select>
                </label>`
              : nothing}
            ${this.historyHasMore
              ? html`<button
                  class="map-ghost"
                  ?disabled=${this.loadingOlder}
                  @click=${() => void this.loadOlder()}
                >
                  ${this.loadingOlder ? MAP_COPY.toolbar.loadingOlder : MAP_COPY.toolbar.olderTurns}
                </button>`
              : nothing}
            <span class="map-toolbar__spacer"></span>
            ${this.live ? html`<span class="map-live"><i></i>${MAP_COPY.toolbar.live}</span>` : nothing}
            <button class="map-ghost" @click=${() => this.openClassic()}>${MAP_COPY.toolbar.classic}</button>
            <button class="map-ghost" @click=${() => this.openEngineer()}>${MAP_COPY.toolbar.engineer}</button>
          </header>

          ${this.renderPending(scene.pending)} ${this.renderSummary(scene)}

          <div class="map-river">
            <div class="map-river__head">
              <span class="map-river__eyebrow">${MAP_COPY.toolbar.river}</span>
              <span class="map-river__hint">${MAP_COPY.toolbar.riverHint}</span>
            </div>
            ${this.loading && scene.lanes.length === 0
              ? html`<div class="map-empty">${MAP_COPY.loading}</div>`
              : scene.lanes.length === 0
                ? html`<div class="map-empty">${this.error || MAP_COPY.empty}</div>`
                : scene.lanes.map((lane, index) => this.renderLane(lane, index))}
            ${scene.hiddenSkills > 0
              ? html`<div class="map-more">${MAP_COPY.hiddenSkills(scene.hiddenSkills)}</div>`
              : nothing}
            ${this.error
              ? html`<div class="map-error">
                  ${this.error}
                  <button class="map-ghost" @click=${() => this.retry()}>${MAP_COPY.retry}</button>
                </div>`
              : nothing}
            ${this.renderFinal(scene)}
          </div>
          ${this.hasNew
            ? html`<button class="map-jump" @click=${() => this.jumpToLatest()}>
                ↓ ${MAP_COPY.jumpLatest}
              </button>`
            : nothing}
        </section>
      </div>
    `;
  }

  private renderPending(pending: ScenePending[]) {
    const question = this.timeline.pendingQuestion;
    const approvals = this.timeline.pendingApprovals;
    if (!question && approvals.length === 0) return nothing;
    const count = approvals.length + (question ? 1 : 0);
    return html`<section class="map-pending">
      <div class="map-pending__head">${MAP_COPY.pendingLabel} <b>${count}</b></div>
      <div class="map-pending__row">
        ${question
          ? html`<question-card
              .sessionKey=${this.sessionKey}
              .pending=${question}
              @answered=${() => this.onQuestionAnswered()}
            ></question-card>`
          : nothing}
        ${approvals.map(
          (approval) => html`<approval-card
            .sessionKey=${this.sessionKey}
            .pending=${approval}
            @answered=${(event: CustomEvent<string>) => this.onApprovalAnswered(event)}
          ></approval-card>`,
        )}
        ${pending.length > 0 && count === 0
          ? html`<div class="map-pending__card">待确认项已处理，等待刷新…</div>`
          : nothing}
      </div>
    </section>`;
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

  private async loadOlder(): Promise<void> {
    if (
      this.loadingOlder ||
      !this.historyHasMore ||
      this.historyBefore === null ||
      !this.sessionKey
    ) {
      return;
    }
    const key = this.sessionKey;
    const before = this.historyBefore;
    this.loadingOlder = true;
    try {
      const page = await fetchSessionEvents(key, 0, 200, { turns: 1, before });
      if (key !== this.sessionKey) return;
      this.timeline.ingest(page);
      const nextBefore = page.nextBefore ?? null;
      this.historyHasMore = Boolean(page.hasMore && nextBefore !== null) && nextBefore !== before;
      this.historyBefore = nextBefore;
      this.rebuild();
    } catch (error) {
      if (key === this.sessionKey) {
        this.error = (error as Error).message || '加载更早轮次失败';
      }
    } finally {
      if (key === this.sessionKey) this.loadingOlder = false;
    }
  }

  private onFlowWheel(event: WheelEvent): void {
    const element = event.currentTarget as HTMLElement;
    if (element.scrollWidth <= element.clientWidth + 1) return;
    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    if (event.shiftKey) return;
    if ((event.target as HTMLElement).closest('.map-card__rows')) return;
    event.preventDefault();
    element.scrollLeft += event.deltaY;
  }

  protected updated(): void {
    this.refreshNowScroll();
  }

  private refreshNowScroll(): void {
    const text = this.querySelector<HTMLElement>('.map-now__text');
    if (!text) {
      if (this.nowScroll.overflow) this.nowScroll = { overflow: false, thumb: 1, top: 0 };
      return;
    }
    const overflow = text.scrollHeight > text.clientHeight + 1;
    const thumb = overflow ? Math.max(0.14, text.clientHeight / text.scrollHeight) : 1;
    const top = overflow
      ? text.scrollTop / Math.max(1, text.scrollHeight - text.clientHeight)
      : 0;
    const changed =
      overflow !== this.nowScroll.overflow ||
      Math.abs(thumb - this.nowScroll.thumb) > 0.005 ||
      Math.abs(top - this.nowScroll.top) > 0.005;
    if (changed) this.nowScroll = { overflow, thumb, top };
  }

  private isNearBottom(): boolean {
    const lanes = this.querySelector<HTMLElement>('.map-lanes');
    if (!lanes) return true;
    return lanes.scrollHeight - lanes.scrollTop - lanes.clientHeight < 140;
  }

  private flagNewContent(): void {
    void this.updateComplete.then(() => {
      if (!this.isNearBottom()) this.hasNew = true;
    });
  }

  private jumpToLatest(): void {
    this.hasNew = false;
    const lanes = this.querySelector<HTMLElement>('.map-lanes');
    lanes?.scrollTo({ top: lanes.scrollHeight, behavior: 'smooth' });
    this.querySelectorAll<HTMLElement>('.map-flow').forEach((flow) => {
      flow.scrollTo({ left: flow.scrollWidth, behavior: 'smooth' });
    });
  }

  private retry(): void {
    if (this.sessionKey) {
      void this.selectSession(this.sessionKey);
    } else {
      void this.bootstrap();
    }
  }

  private toggleNotes(laneId: string): void {
    const next = new Set(this.openNotes);
    if (next.has(laneId)) next.delete(laneId);
    else next.add(laneId);
    this.openNotes = next;
  }

  private openNote(laneId: string, note: SceneNote): void {
    const next = new Set(this.openNotes);
    next.add(laneId);
    this.openNotes = next;
    const key = `${laneId}:${note.at}`;
    this.noteFocus = key;
    void this.updateComplete.then(() => {
      this.querySelector('.map-notes__item.is-focus')?.scrollIntoView({
        block: 'nearest',
        behavior: 'smooth',
      });
    });
    window.setTimeout(() => {
      if (this.noteFocus === key) this.noteFocus = null;
    }, 2400);
  }

  private onLanesScroll(): void {
    if (this.hasNew && this.isNearBottom()) this.hasNew = false;
  }

  private onFlowKey(event: KeyboardEvent): void {
    const flow = event.currentTarget as HTMLElement;
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const step = Math.max(140, flow.clientWidth * 0.55);
    flow.scrollLeft += event.key === 'ArrowRight' ? step : -step;
  }

  private onShellClick(event: MouseEvent): void {
    const target = event.target as HTMLElement;
    if (target.closest('.map-card__name') || target.closest('.map-card__sub')) return;
    this.tip = null;
  }

  private async refreshAfterApproval(): Promise<void> {
    if (!this.sessionKey) return;
    const key = this.sessionKey;
    try {
      const page = await fetchSessionEvents(key, this.timeline.highWatermark, 200);
      if (key !== this.sessionKey) return;
      this.timeline.ingest(page);
      this.rebuild();
    } catch (error) {
      console.error('story-map refresh after approval failed', error);
    }
  }

  private renderSummary(scene: StoryScene) {
    return html`<section class="map-summary">
      <div class="map-summary__seal">✓</div>
      <div class="map-summary__body">
        <div class="map-summary__label">${MAP_COPY.summaryLabel} · ${scene.summary.status}</div>
        <p class="map-summary__text">${scene.summary.text}</p>
        ${scene.summary.skills.length > 0
          ? html`<div class="map-summary__agenda">${MAP_COPY.skillsPrefix}${scene.summary.skills.join(' · ')}</div>`
          : nothing}
      </div>
    </section>`;
  }

  private renderLane(lane: SceneLane, index: number) {
    const notesAt = (position: number) => lane.notes.filter((note) => note.index === position);
    const notesOpen = lane.prologue || this.openNotes.has(lane.id);
    return html`<section class="map-lane" data-lane=${index}>
      <div class="map-lane__head">
        <span class="map-lane__no">${String(index + 1).padStart(2, '0')}</span>
        <span class="map-lane__time">${formatClock(lane.at)}</span>
        <span class="map-lane__status is-${lane.status}">
          ${MAP_COPY.laneStatus[lane.status]}
        </span>
        <span class="map-lane__count">${lane.steps} 步</span>
        ${lane.notes.length > 0 && !lane.prologue
          ? html`<button class="map-lane__notes" @click=${() => this.toggleNotes(lane.id)}>
              <span>${MAP_COPY.notesButton(lane.notes.length)}</span>
              <b>${notesOpen ? '▾' : '▸'}</b>
            </button>`
          : lane.prologue && lane.notes.length > 0
            ? html`<span class="map-lane__notes is-static">
                ${MAP_COPY.prologueNotes(lane.notes.length)}
              </span>`
            : nothing}
      </div>
      ${notesOpen && lane.notes.length > 0 ? this.renderNotes(lane) : nothing}
      ${lane.cards.length > 0
        ? html`<div
            class="map-flow"
            role="list"
            tabindex="0"
            aria-label=${`${lane.skill}，左右方向键滚动`}
            @wheel=${(event: WheelEvent) => this.onFlowWheel(event)}
            @keydown=${(event: KeyboardEvent) => this.onFlowKey(event)}
          >
            ${lane.cards.map((card, cardIndex) => {
              const note = notesAt(cardIndex)[0];
              return html`${cardIndex === 0
                ? nothing
                : html`<span class="map-edge"><i>${edgeLabel(card.kind)}</i><b></b></span>`}${this.renderCard(
                card,
                note ? () => this.openNote(lane.id, note) : undefined,
              )}`;
            })}
          </div>`
        : nothing}
    </section>`;
  }

  private renderNotes(lane: SceneLane) {
    return html`<div class="map-notes">
      <div class="map-notes__label">${MAP_COPY.noteTitle}</div>
      ${lane.notes.map((note) => {
        const focused = this.noteFocus === `${lane.id}:${note.at}`;
        return html`<div class=${`map-notes__item ${focused ? 'is-focus' : ''}`}>
          <span class="map-notes__at">${formatClock(note.at)}</span>
          <div class="map-notes__body">
            <p class="map-notes__text">${note.text}</p>
            ${note.nextCardTitle
              ? html`<div class="map-notes__next">
                  → ${MAP_COPY.noteNext}：${note.nextCardTitle}
                </div>`
              : nothing}
          </div>
        </div>`;
      })}
    </div>`;
  }

  private renderCard(card: SceneCard, onNote?: () => void) {
    if (card.items && card.items.length > 1) {
      return html`<div
        class="map-stack"
        style=${`--n:${card.items.length}`}
        tabindex="0"
        role="listitem"
        aria-label=${`${card.items.length} 张同类卡片`}
      >
        <span class="map-stack__badge">+${card.items.length - 1}</span>
        ${card.items.map(
          (item, itemIndex) =>
            html`<div class="map-stack__item" style=${`--i:${itemIndex}`}>
              ${this.renderCardBody(item, itemIndex > 2 ? 'is-ghost' : '')}
            </div>`,
        )}
      </div>`;
    }
    return this.renderCardBody(card, '', onNote);
  }

  private renderCardBody(card: SceneCard, extra: string, onNote?: () => void) {
    const kind = card.kind;
    const statusText =
      card.status === 'running'
        ? MAP_COPY.laneStatus.running
        : card.status === 'failed'
          ? MAP_COPY.laneStatus.failed
          : '';
    const label = `${card.title}${statusText ? `，${statusText}` : ''}${
      card.repeatCount && card.repeatCount > 1 ? `，连续 ${card.repeatCount} 次` : ''
    }`;
    return html`<article
      class=${`map-card map-card--${kind} ${extra}`}
      tabindex="0"
      role=${extra === 'is-ghost' ? 'presentation' : 'listitem'}
      aria-label=${label}
    >
      ${kind === 'vision' && card.images?.[0]
        ? html`<div class="map-card__media">
            <evidence-image
              .evidenceId=${card.images[0].evidenceId}
              .workspaceId=${this.workspaceId}
              .alt=${`${card.title} ${card.sub}`}
            ></evidence-image>
          </div>`
        : nothing}
      <header class="map-card__head">
        <span class="map-dot ${kind === 'finding' || kind === 'error' ? 'is-warn' : ''}"></span>
        <span
          class="map-card__name"
          @mouseenter=${(event: Event) => this.showTip(event, card.title)}
          @mouseleave=${() => this.hideTip()}
          @click=${(event: Event) => this.toggleTip(event, card.title)}
          >${card.title}</span
        >
        ${card.repeatCount && card.repeatCount > 1
          ? html`<span class="map-card__repeat" title=${`连续调用 ${card.repeatCount} 次`}
              >×${card.repeatCount}</span
            >`
          : nothing}
        ${statusText
          ? html`<span class=${`map-pill ${card.status === 'failed' ? 'is-warn' : 'is-muted'}`}>${statusText}</span>`
          : kind === 'finding'
            ? html`<span class="map-pill is-warn">发现</span>`
            : nothing}
      </header>
      <div
        class="map-card__sub"
        @mouseenter=${(event: Event) => this.showTip(event, card.sub)}
        @mouseleave=${() => this.hideTip()}
        @click=${(event: Event) => this.toggleTip(event, card.sub)}
      >
        ${card.sub}
      </div>
      ${card.params && card.params.length > 0
        ? html`<div
            class="map-card__params"
            @mouseenter=${(event: Event) => this.showTip(event, cardParams(card))}
            @mouseleave=${() => this.hideTip()}
            @click=${(event: Event) => this.toggleTip(event, cardParams(card))}
          >
            ${cardParams(card)}
          </div>`
        : nothing}
      ${card.metrics.length > 0
        ? html`<div class="map-card__rule"></div>
            <div class="map-card__rows">
              ${card.metrics.map(
                (metric) => html`<div class="map-row">
                  <span>${metric.label}</span><b>${metric.value}</b>
                </div>`,
              )}
            </div>`
        : nothing}
      ${kind === 'conclusion' && card.detail
        ? html`<div class="map-card__rule"></div>
            <p class="map-card__text">${card.detail}</p>`
        : nothing}
      <footer class="map-card__foot">
        ${onNote
          ? html`<button
              class="map-card__why"
              @click=${(event: Event) => {
                event.stopPropagation();
                onNote();
              }}
            >
              ${MAP_COPY.noteButton}
            </button>`
          : nothing}
        <span
          class="map-card__tool"
          @mouseenter=${(event: Event) => card.tool && this.showTip(event, card.tool)}
          @mouseleave=${() => this.hideTip()}
          @click=${(event: Event) => card.tool && this.toggleTip(event, card.tool)}
          >${card.tool || ''}</span
        >
        <span class="map-card__time">${card.at ? formatClock(card.at) : ''}</span>
      </footer>
    </article>`;
  }

  private renderFinal(scene: StoryScene) {
    const final = scene.final;
    if (!final) return nothing;
    return html`<section class="map-final">
      <header>
        <span class="map-final__eyebrow">${MAP_COPY.finalEyebrow}</span>
        <span class="map-final__time">${formatClock(final.at)}</span>
        <span class=${`map-pill ${final.tone === 'warn' ? 'is-warn' : 'is-muted'}`}>${final.status}</span>
      </header>
      <p class="map-final__text">${final.text}</p>
      <dl class="map-final__cols">
        ${final.cols.map(
          ([label, a, b]) => html`<div><dt>${label}</dt><dd>${a}<br />${b}</dd></div>`,
        )}
      </dl>
      <div class="map-final__foot">
        <span>${final.meta}</span>
        <span class="map-final__btns">
          <button
            class="map-ghost"
            @click=${() => (this.answerOpen = !this.answerOpen)}
          >
            ${this.answerOpen ? MAP_COPY.hideAnswer : MAP_COPY.viewAnswer}
          </button>
        </span>
      </div>
      ${this.answerOpen
        ? html`<div class="map-final__answer"><p>${final.fullText}</p></div>`
        : nothing}
    </section>`;
  }

  /* ── 语音球 + 连线 ───────────────────────────────────── */

}

function cardParams(card: SceneCard): string {
  return (card.params ?? []).map((param) => `${param.label} ${param.value}`).join(' · ');
}

function sectionStatusLabel(status: string): string {
  if (status === 'running') return '进行中';
  if (status === 'failed') return '失败';
  if (status === 'partial') return '部分完成';
  if (status === 'cancelled') return '已取消';
  return '已完成';
}

function edgeLabel(kind: SceneCard['kind']): string {
  switch (kind) {
    case 'device':
      return '读取';
    case 'vision':
      return '画面';
    case 'think':
      return '判断';
    case 'finding':
      return '发现';
    case 'action':
      return '处置';
    case 'plan':
      return '计划';
    case 'subagent':
      return '派生';
    case 'conclusion':
      return '结论';
    case 'error':
      return '失败';
    case 'list':
      return '清单';
    case 'doc':
      return '查阅';
    default:
      return '继续';
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-story-map': StoryMapView;
  }
}
