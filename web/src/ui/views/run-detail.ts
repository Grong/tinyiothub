/**
 * run 详情页（AI 运行透明化 T13）。
 *
 * 路由：`#/ai-ops/runs/{id}`（app.ts hash 路由；id 容忍冒号，如 patrol:p1——
 * patrol 提案兜底用 brain event id，T12 评审 M-1 结转）。
 *
 * 数据源（两段式）：
 * 1. `fetchRunTrace(id)` —— 主源（进行中 live / 已结束 report trace）；
 * 2. 404（TraceNotFoundError）→ 退 `brainEventApi.detail(id)`——覆盖已结束
 *    judgment 与 patrol 提案（detail.trace + narrative + evidence）。
 * 进行中（live:true）继续 3s 轮询，代际守卫 + AbortController（卡片层同款模式），
 * 翻 false 自然停；离开页面（disconnected）停轮询 + abort 在飞请求。
 *
 * 层级（设计裁定）：顶部元信息条（状态/工具数/耗时）→ 时间线主视觉（full 模式）
 * → narrative/正文 → verdict 原文折叠 → think 块折叠（D2）。
 * 三态：loading=骨架屏（3 行灰占位）；双源 404=「该运行记录不存在或已过期」+ 返回；
 * 空 trace=「本次运行未调用工具」+ 元信息条（D4）。h1=标题 + 「← 返回」（D8）。
 * 导出 JSON=组装 { meta, entries } 写剪贴板（扩展 #8）。
 *
 * 已知取舍：trace/detail 两端点均无 tokens 字段（RunReport 有，端点未透出），
 * 元信息条不渲染 tokens 格——沿用 T12「完成态无预算头，不补后端」裁定。
 * 完成态时间线 budgetMax 传 0 隐藏预算头（detail 响应无 budgetMax）。
 *
 * S3 纯文本铁律：trace/verdict/think/narrative 一律 lit 文本绑定（自动转义），
 * 禁 md()/unsafeHTML/innerHTML。
 */
import { LitElement, html, nothing, type PropertyValues, type TemplateResult } from "lit";
import { customElement, property, state } from "lit/decorators.js";
import { fetchRunTrace, type RunTrace, type TraceEntry } from "../../api/agent-runs.js";
import { brainEventApi, type BrainEventDetail } from "../../api/brain-events.js";
import { renderRunTimeline } from "../shared/run-timeline.js";
import { evidenceRawText, segmentEvidence } from "../shared/evidence-segments.js";
import { success as toastSuccess, error as toastError } from "../components/toast.js";
import "./run-detail.css";

/** live 轮询节奏与有限重试上限（与卡片层 TRACE_POLL_MS/TRACE_MAX_RETRIES 一致）。 */
const POLL_MS = 3_000;
const POLL_MAX_RETRIES = 3;

type PageState = "loading" | "ready" | "not-found";

/** 耗时文案（导出供测试）：<60s "x 秒"，否则 "x 分 y 秒"；非法/负值 → null。 */
export function fmtDuration(startIso: string, endIso: string): string | null {
  const ms = new Date(endIso).getTime() - new Date(startIso).getTime();
  if (!Number.isFinite(ms) || ms < 0) return null;
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} 秒`;
  return `${Math.floor(s / 60)} 分 ${s % 60} 秒`;
}

@customElement("view-run-detail")
export class RunDetailView extends LitElement {
  @property() runId = "";

  @state() private pageState: PageState = "loading";
  @state() private trace: RunTrace | null = null;
  @state() private detail: BrainEventDetail | null = null;

  /** 代际守卫：load/轮询共用；切 runId/卸载 bump 后旧异步结果直接丢弃。 */
  private gen = 0;
  private abort: AbortController | null = null;
  private pollTimer: number | null = null;

  /** Light DOM：run-detail.css 与全局令牌（--text/--border/--space-*）直接生效。 */
  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    if (this.runId) void this.load();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.gen += 1;
    this.stopPolling();
  }

  willUpdate(changed: PropertyValues<this>) {
    // 同页切换 runId（有旧值才算切换；初始赋值由 connectedCallback 的 load 承接）
    if (changed.has("runId") && changed.get("runId") !== undefined && this.runId) {
      this.stopPolling();
      void this.load();
    }
  }

  private get entries(): TraceEntry[] {
    return this.trace?.entries ?? this.detail?.trace ?? [];
  }

  private get live(): boolean {
    return this.trace?.live ?? false;
  }

  private get titleText(): string {
    return this.detail?.title || "运行详情";
  }

  /** 耗时：仅 detail 兜底路径可推导（resolvedAt ?? judgedAt − createdAt）。 */
  private get duration(): string | null {
    if (!this.detail) return null;
    const end = this.detail.resolvedAt ?? this.detail.judgedAt;
    return end ? fmtDuration(this.detail.createdAt, end) : null;
  }

  private async load() {
    const gen = ++this.gen;
    this.abort = new AbortController();
    this.pageState = "loading";
    this.trace = null;
    this.detail = null;
    try {
      const t = await fetchRunTrace(this.runId, this.abort.signal);
      if (gen !== this.gen) return;
      this.trace = t;
      this.pageState = "ready";
      if (t.live) this.startPolling(gen);
    } catch {
      if (gen !== this.gen) return;
      // 404（或 trace 端点其它失败）→ 退 brain-event detail（已结束 judgment / patrol 提案）
      try {
        const d = await brainEventApi.detail(this.runId);
        if (gen !== this.gen) return;
        this.detail = d;
        this.pageState = "ready";
      } catch {
        if (gen !== this.gen) return;
        this.pageState = "not-found";
      }
    }
  }

  /** live 轮询：3s 节奏 + 代际守卫 + AbortController；失败有限重试 3 次后静默停
   *  （保留最后一次快照；详情页无「轨迹不可用」文案裁定，卡片层有）。 */
  private startPolling(gen: number) {
    let retries = 0;
    const tick = async () => {
      try {
        const t = await fetchRunTrace(this.runId, this.abort?.signal);
        if (gen !== this.gen) return;
        retries = 0;
        this.trace = t;
        if (!t.live) return; // 翻完成态：自然停
      } catch {
        if (gen !== this.gen) return;
        retries += 1;
        if (retries > POLL_MAX_RETRIES) return;
      }
      this.pollTimer = window.setTimeout(() => void tick(), POLL_MS);
    };
    this.pollTimer = window.setTimeout(() => void tick(), POLL_MS);
  }

  private stopPolling() {
    this.abort?.abort();
    this.abort = null;
    if (this.pollTimer !== null) {
      window.clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }
  }

  /** 导出 JSON（扩展 #8）：组装 { meta, entries } 写剪贴板。 */
  private async exportJson() {
    const payload = {
      meta: {
        id: this.runId,
        title: this.titleText,
        live: this.live,
        toolCount: this.entries.length,
        exportedAt: new Date().toISOString(),
      },
      entries: this.entries,
    };
    try {
      await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
      toastSuccess("已复制到剪贴板");
    } catch {
      toastError("复制失败，请重试");
    }
  }

  private navigateBack(e: Event) {
    e.preventDefault();
    window.history.pushState({}, "", "/dispositions");
    window.dispatchEvent(new PopStateEvent("popstate"));
  }

  private renderBackLink(): TemplateResult {
    return html`<a class="rd-back" href="/dispositions" @click=${this.navigateBack}>← 返回</a>`;
  }

  private renderSkeleton(): TemplateResult {
    return html`<div class="rd-skeleton" aria-busy="true" aria-label="加载中">
      <div class="skeleton skeleton-title"></div>
      <div class="skeleton skeleton-text"></div>
      <div class="skeleton skeleton-text"></div>
      <div class="skeleton skeleton-text"></div>
    </div>`;
  }

  private renderNotFound(): TemplateResult {
    return html`<div class="rd-notfound">
      <p>该运行记录不存在或已过期</p>
      ${this.renderBackLink()}
    </div>`;
  }

  private renderMeta(): TemplateResult {
    const duration = this.duration;
    return html`<div class="rd-meta">
      <span class="rd-meta-item rd-status ${this.live ? "rd-status--live" : ""}">
        ${this.live ? "进行中" : "已结束"}
      </span>
      <span class="rd-meta-item">工具 ${this.entries.length} 个</span>
      ${duration ? html`<span class="rd-meta-item">耗时 ${duration}</span>` : nothing}
      <span class="rd-meta-item rd-runid">${this.runId}</span>
    </div>`;
  }

  private renderReady(): TemplateResult {
    const entries = this.entries;
    // verdict/think 切分（D2）：仅 detail 兜底路径有 evidence
    const raw = this.detail ? evidenceRawText(this.detail.evidence) : "";
    const segments = raw ? segmentEvidence(raw) : [];
    const texts = segments.filter((s) => s.kind === "text");
    const verdicts = segments.filter((s) => s.kind === "verdict");
    const thinks = segments.filter((s) => s.kind === "think");
    return html`
      <header class="rd-head">
        ${this.renderBackLink()}
        <h1 class="rd-title">${this.titleText}</h1>
        <button class="btn btn--ghost btn--sm rd-export" @click=${() => void this.exportJson()}>
          导出 JSON
        </button>
      </header>
      ${this.renderMeta()}
      ${entries.length === 0
        ? html`<div class="rd-empty">本次运行未调用工具</div>`
        : renderRunTimeline({
            entries,
            live: this.live,
            // 完成态无预算头（detail 无 budgetMax；T12 评审已记录，不补后端）
            budgetMax: this.live ? (this.trace?.budgetMax ?? 0) : 0,
            mode: "full",
          })}
      ${this.detail?.narrative ? html`<p class="rd-narrative">${this.detail.narrative}</p>` : nothing}
      ${texts.map((s) => html`<p class="rd-text">${s.text}</p>`)}
      ${verdicts.map(
        (s) => html`<details class="rd-fold">
          <summary>verdict 原文</summary>
          <pre class="rd-pre">${s.text}</pre>
        </details>`,
      )}
      ${thinks.map(
        (s) => html`<details class="rd-fold">
          <summary>思考过程</summary>
          <pre class="rd-pre">${s.text}</pre>
        </details>`,
      )}
    `;
  }

  render() {
    if (this.pageState === "loading") return this.renderSkeleton();
    if (this.pageState === "not-found") return this.renderNotFound();
    return this.renderReady();
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "view-run-detail": RunDetailView;
  }
}
