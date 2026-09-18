/**
 * 处置中心（AI 大脑主干化 P0）— AI 大脑事件 feed（/brain-events）。
 *
 * 设计规格（design-review v6 线框稿定稿）：
 * - 单色纪律：颜色只出现在「需要行动」处（琥珀 ● + 白底主按钮），无渐变发光。
 * - 每个事件 = 独立卡片，两行结构（标题行 + 理由行；需行动的理由不截断）。
 * - tab 默认「需要你」；「全部/巡检/报警/指令」按来源过滤，留给审计回看。
 * - 徽章渲染用 BADGE_MAP[ev.status]（覆盖全部状态词，含 patrol_ok）。
 * - 变更路由（P0）：alarm 源走 /judgments，patrol 源走 /heartbeat/approvals；
 *   动作审批（批准/拒绝）在卡片动作区；判断反馈（✓/✕）仅 alarm 源——
 *   审批只影响处置流不写 feedback（F14）；alarm 拒绝必填原因；点错必填原因。
 * - 列表行无 evidence：证据懒加载，展开才调 /brain-events/{id}。
 * - 空态 = 产品故事时刻：「一切正常」一句话 + 摘要入口。
 * - SSE：judgment_judged/judgment_updated → 刷新列表与摘要。
 */

import { LitElement, html, nothing, type TemplateResult } from "lit";
import { customElement, state } from "lit/decorators.js";
import {
  brainEventApi,
  brainEventTargetId,
  type BrainEvent,
  type BrainEventsSummary,
  type BrainEventStatus,
  type BrainEventTab,
} from "../../api/brain-events.js";
import { judgmentApi } from "../../api/judgments.js";
import { connectSse, type SseConnection } from "../../api/sse-client.js";
import { success, error as toastError } from "../components/toast.js";
import "./dispositions.css";

/** F12 页大小（四处复用同一常量，防漂移） */
const PAGE_SIZE = 20;

const TAB_LABELS: Record<BrainEventTab, string> = {
  needs_you: "需要你",
  all: "全部",
  patrol: "巡检",
  alarm: "报警",
  directive: "指令",
};

/** 状态徽章（导出供测试）：覆盖 brain_events 全部状态词，含 patrol_tick 行的 patrol_ok。 */
export const BADGE_MAP: Record<BrainEventStatus, string> = {
  investigating: "分析中",
  self_closed: "自行闭环",
  awaiting_approval: "需要你",
  executing: "执行中",
  resolved: "闭环",
  escalated: "转工单",
  investigation_failed: "转工单",
  dismissed: "已拒绝",
  budget_skipped: "未受理",
  dispatch_suppressed: "未受理",
  patrol_ok: "巡检正常",
};

/** 「需要你」= 待审批 + 需人工（F3/F15①，导出供测试）。 */
export function needsYou(ev: BrainEvent): boolean {
  return ev.status === "awaiting_approval" || ev.status === "escalated";
}

/** tab 角标计数（导出供测试）：仅「需要你」有角标，来自 summary.needsYou。 */
export function tabCount(summary: BrainEventsSummary | null, tab: BrainEventTab): number | null {
  if (tab !== "needs_you" || !summary) return null;
  return summary.needsYou;
}

/** 时间戳规则（F12⑤，导出供测试）：1h 内"x 分钟前"、当天 HH:MM、跨天 M月D日。 */
export function fmtJudgmentTime(iso: string, now = Date.now()): string {
  const d = new Date(iso);
  const diffMin = Math.floor((now - d.getTime()) / 60_000);
  if (diffMin < 1) return "刚刚";
  if (diffMin < 60) return `${diffMin} 分钟前`;
  if (d.toDateString() === new Date(now).toDateString()) {
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  }
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}

/** 审批倒计时文案（F6，导出供测试）：非待审批/无 judgedAt → null。
 *  P0：/brain-events/summary 不下发 approvalTimeoutHours，用默认 24h。 */
export function approvalCountdown(ev: BrainEvent, now = Date.now(), timeoutHours = 24): string | null {
  if (ev.status !== "awaiting_approval" || !ev.judgedAt) return null;
  const deadline = new Date(ev.judgedAt).getTime() + timeoutHours * 3600_000;
  const remainH = Math.max(0, Math.round((deadline - now) / 3600_000));
  return `${remainH} 小时后自动转工单`;
}

/** 证据段落（导出供测试）：think = 思考块，text = 正文，verdict = 尾部协议块原文。 */
export interface EvidenceSegment {
  kind: "think" | "text" | "verdict";
  text: string;
}

/** 证据原文分段（导出供测试）。完整记录不丢弃：think 块与 verdict 协议块
 *  各自成段（渲染层折叠展示），正文按原顺序保留。 */
export function segmentEvidence(raw: string): EvidenceSegment[] {
  // 先摘尾部 verdict 协议块（调查指令约定在末尾；完整保留，折叠展示）
  let body = raw;
  let verdict: string | null = null;
  const fenceStart = body.lastIndexOf("```json");
  if (fenceStart >= 0) {
    const fenceEnd = body.indexOf("```", fenceStart + 7);
    if (fenceEnd >= 0) {
      verdict = body.slice(fenceStart + 7, fenceEnd).trim() || null;
      body = (body.slice(0, fenceStart) + body.slice(fenceEnd + 3)).trim();
    }
  }
  const segments: EvidenceSegment[] = [];
  const thinkRe = /<think>([\s\S]*?)(?:<\/think>|$)/g; // 兼容未闭合尾块
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = thinkRe.exec(body)) !== null) {
    const before = body.slice(last, m.index).trim();
    if (before) segments.push({ kind: "text", text: before });
    const think = m[1].trim();
    if (think) segments.push({ kind: "think", text: think });
    last = m.index + m[0].length;
  }
  const tail = body.slice(last).trim();
  if (tail) segments.push({ kind: "text", text: tail });
  if (verdict) segments.push({ kind: "verdict", text: verdict });
  return segments;
}

/** 证据原文提取（导出供测试）：新行读 summary（完整记录），存量老行读 excerpt。 */
export function evidenceRawText(evidence: unknown): string {
  if (!evidence || typeof evidence !== "object") return "";
  const ev = evidence as { summary?: unknown; excerpt?: unknown };
  if (typeof ev.summary === "string") return ev.summary;
  if (typeof ev.excerpt === "string") return ev.excerpt;
  return "";
}

/** 反馈校验（F13，导出供测试）：点错必填 ≥4 字符。 */
export function validateWrongReason(reason: string): string | null {
  return reason.trim().length >= 4 ? null : "点错必须填写原因（至少 4 个字符）";
}

/** 行点击展开/收起（导出供测试）：点同一行收起，点他行切换。 */
export function nextExpandedId(current: string | null, clickedId: string): string | null {
  return current === clickedId ? null : clickedId;
}

/** 空态产品文案（导出供测试）：需要你 = 「一切正常」一句话 + 今日摘要；
 *  全部（及其余过滤 tab）= 「还没有事件——大脑还没开始干活」。 */
export function emptyStateCopy(
  tab: BrainEventTab,
  summary: BrainEventsSummary | null,
): { title: string; body: string } {
  if (tab === "needs_you") {
    return {
      title: "一切正常",
      body: summary
        ? `今日 ${summary.digestedToday} 条已消化，没有需要你处理的事`
        : "没有需要你处理的事",
    };
  }
  return { title: "还没有事件", body: "大脑还没开始干活" };
}

@customElement("view-dispositions")
export class DispositionsView extends LitElement {
  @state() private events: BrainEvent[] = [];
  @state() private summary: BrainEventsSummary | null = null;
  @state() private tab: BrainEventTab = "needs_you";
  @state() private loading = true;
  @state() private loadError: string | null = null;
  @state() private expandedId: string | null = null;
  @state() private wrongPanelId: string | null = null;
  @state() private wrongReason = "";
  @state() private rejectPanelId: string | null = null;
  @state() private rejectReason = "";
  @state() private hasMore = false;
  /** 证据懒加载缓存（列表行无 evidence）：event id → detail 端点取回的 evidence */
  @state() private evidenceById = new Map<string, unknown>();
  /** 证据加载失败的 event id（内联「证据加载失败，重试」，不弹 toast） */
  @state() private evidenceFailedIds = new Set<string>();
  /** SSE 断连细条（仿 tickets.ts）：onError 置 false，load 成功复位 */
  @state() private sseConnected = true;
  /** 操作防重入：进行中的 event id（快速连点/重复点击不重复提交） */
  @state() private pendingIds = new Set<string>();

  private sseConn: SseConnection | null = null;
  private poller: number | null = null;

  /** Light DOM：dispositions.css 与全局 .btn 样式才能生效（与 tickets/alarms 一致） */
  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    void this.load();
    this.sseConn = connectSse(
      "/api/v1/workspaces/notifications/stream",
      (event) => {
        if (event === "judgment_judged" || event === "judgment_updated" || event === "ticket_created") {
          void this.load(true);
        }
      },
      () => {
        this.sseConnected = false;
      },
    );
    this.poller = window.setInterval(() => void this.load(true), 60_000);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.sseConn?.close();
    if (this.poller !== null) window.clearInterval(this.poller);
  }

  private async load(quiet = false) {
    if (!quiet) this.loading = true;
    this.loadError = null;
    try {
      const [events, summary] = await Promise.all([
        brainEventApi.list(this.tab, undefined, PAGE_SIZE),
        brainEventApi.summary(),
      ]);
      this.events = events;
      this.summary = summary;
      this.sseConnected = true;
      // 游标分页（F12）：满页即可能还有
      this.hasMore = events.length >= PAGE_SIZE;
    } catch (e) {
      this.loadError = e instanceof Error ? e.message : "加载失败";
    } finally {
      this.loading = false;
    }
  }

  /** F12：游标式「加载更多」（后续页纯时间流，折叠/置顶仅首页——T-13/C2）。 */
  private async loadMore() {
    const last = this.events[this.events.length - 1];
    if (!last) return;
    try {
      const more = await brainEventApi.list(this.tab, last.id, PAGE_SIZE);
      this.events = [...this.events, ...more];
      this.hasMore = more.length >= PAGE_SIZE;
    } catch (e) {
      toastError(e instanceof Error ? e.message : "加载失败");
    }
  }

  private switchTab(tab: BrainEventTab) {
    if (tab === this.tab) return;
    this.tab = tab;
    this.expandedId = null;
    this.wrongPanelId = null;
    this.rejectPanelId = null;
    this.evidenceById = new Map();
    void this.load();
  }

  /** 防重入包装：同一 event 的操作在飞行中时忽略后续触发，
   *  直到刷新落地（列表状态翻转）才解除。 */
  private async runPending(id: string, fn: () => Promise<void>) {
    if (this.pendingIds.has(id)) return;
    this.pendingIds = new Set(this.pendingIds).add(id);
    try {
      await fn();
    } finally {
      const next = new Set(this.pendingIds);
      next.delete(id);
      this.pendingIds = next;
    }
  }

  /** 可展开证据的行：分析中（无结论）与巡检正常聚合行（跳巡检历史）除外。 */
  private canEvidence(ev: BrainEvent): boolean {
    return ev.status !== "investigating" && ev.status !== "patrol_ok";
  }

  /** 行点击：巡检正常聚合行（patrol_tick）跳 AI 运维巡检历史；其余可证据行展开/收起。 */
  private onRowClick(ev: BrainEvent) {
    if (ev.source === "patrol_tick") {
      window.location.hash = "#/ai-ops";
      return;
    }
    if (!this.canEvidence(ev)) return;
    void this.toggleEvidence(ev);
  }

  /** 证据懒加载：展开时若未缓存则调 detail 端点取 evidence；失败内联记失败态。 */
  private async toggleEvidence(ev: BrainEvent) {
    const next = nextExpandedId(this.expandedId, ev.id);
    this.expandedId = next;
    if (next === null || this.evidenceById.has(ev.id)) return;
    if (this.evidenceFailedIds.delete(ev.id)) {
      this.evidenceFailedIds = new Set(this.evidenceFailedIds);
    }
    try {
      const detail = await brainEventApi.detail(ev.id);
      this.evidenceById = new Map(this.evidenceById).set(ev.id, detail.evidence);
    } catch {
      this.evidenceFailedIds = new Set(this.evidenceFailedIds).add(ev.id);
    }
  }

  private async vote(ev: BrainEvent, verdict: "right" | "wrong") {
    if (verdict === "wrong") {
      this.wrongPanelId = this.wrongPanelId === ev.id ? null : ev.id;
      this.wrongReason = "";
      return;
    }
    await this.runPending(ev.id, async () => {
      try {
        await judgmentApi.feedback(brainEventTargetId(ev), "right");
        success("已记录：AI 判断正确");
        await this.load(true);
      } catch (e) {
        toastError(e instanceof Error ? e.message : "反馈失败");
      }
    });
  }

  private async submitWrong(ev: BrainEvent) {
    const err = validateWrongReason(this.wrongReason);
    if (err) {
      toastError(err);
      return;
    }
    await this.runPending(ev.id, async () => {
      try {
        await judgmentApi.feedback(brainEventTargetId(ev), "wrong", this.wrongReason.trim());
        success("已写入 AI 的记忆，下次它会做得更好");
        this.wrongPanelId = null;
        await this.load(true);
      } catch (e) {
        toastError(e instanceof Error ? e.message : "反馈失败");
      }
    });
  }

  private async approve(ev: BrainEvent) {
    await this.runPending(ev.id, async () => {
      try {
        await brainEventApi.approve(ev);
        success("已批准，开始执行");
        await this.load(true);
      } catch (e) {
        toastError(e instanceof Error ? e.message : "批准失败");
      }
    });
  }

  /** alarm 源拒绝：必填原因（写进工单）；patrol 源拒绝：直接提交（端点不收原因）。 */
  private async reject(ev: BrainEvent) {
    if (ev.source !== "patrol") {
      this.rejectPanelId = this.rejectPanelId === ev.id ? null : ev.id;
      this.rejectReason = "";
      return;
    }
    await this.runPending(ev.id, async () => {
      try {
        await brainEventApi.reject(ev);
        success("已拒绝");
        await this.load(true);
      } catch (e) {
        toastError(e instanceof Error ? e.message : "拒绝失败");
      }
    });
  }

  private async submitReject(ev: BrainEvent) {
    const reason = this.rejectReason.trim();
    if (reason.length < 4) {
      toastError("拒绝必须填写原因（至少 4 个字符）");
      return;
    }
    await this.runPending(ev.id, async () => {
      try {
        await brainEventApi.reject(ev, reason);
        success("已拒绝并转工单");
        this.rejectPanelId = null;
        await this.load(true);
      } catch (e) {
        toastError(e instanceof Error ? e.message : "拒绝失败");
      }
    });
  }


  private renderHeader(): TemplateResult {
    const s = this.summary;
    const feedbackTotal = s ? s.feedbackRight + s.feedbackWrong : 0;
    // 页面标题由 app shell 的 content-header 统一渲染，这里只保留统计数据行
    return html`
      <div class="disp-head">
        <p class="disp-stats">
          ${s ? html`今日 ${s.digestedToday} 条已消化 · ${s.needsYou} 条需要你` : "…"}
          ${s?.latencyP50Secs != null
            ? html` · 判断延迟 p50 ${Math.round(s.latencyP50Secs / 60)} 分钟`
            : nothing}
        </p>
        ${s && feedbackTotal > 0
          ? html`<p class="disp-learn">AI 已从你的 ${feedbackTotal} 条反馈中学习
              （${s.feedbackRight} 对 / ${s.feedbackWrong} 错）</p>`
          : nothing}
      </div>
    `;
  }

  private renderTabs(): TemplateResult {
    return html`
      <div class="disp-tabs" role="tablist">
        ${(Object.keys(TAB_LABELS) as BrainEventTab[]).map((t) => {
          const count = tabCount(this.summary, t);
          return html`
            <button
              role="tab"
              aria-selected=${t === this.tab}
              class="disp-tab ${t === this.tab ? "active" : ""}"
              @click=${() => this.switchTab(t)}
            >
              ${TAB_LABELS[t]}${count !== null && count > 0 ? html`<span class="count">${count}</span>` : nothing}
            </button>
          `;
        })}
      </div>
    `;
  }

  /** 证据面板：完整审计记录的结构化呈现——正文直展，think/verdict 原文折叠，
   *  动作记录与 run 元信息收尾。 */
  private renderEvidencePanel(evidence: unknown): TemplateResult {
    const ev = (evidence && typeof evidence === "object" ? evidence : {}) as {
      actions?: unknown;
      tool_calls?: unknown;
      duration_ms?: unknown;
      tokens?: unknown;
    };
    const segments = segmentEvidence(evidenceRawText(evidence));
    const actions = Array.isArray(ev.actions) ? ev.actions : [];
    if (segments.length === 0 && actions.length === 0) {
      return html`<div class="j-evidence">暂无证据</div>`;
    }
    const meta = [
      typeof ev.tool_calls === "number" && ev.tool_calls > 0 ? `工具调用 ${ev.tool_calls} 次` : null,
      typeof ev.duration_ms === "number" ? `耗时 ${(ev.duration_ms / 1000).toFixed(1)}s` : null,
      typeof ev.tokens === "number" && ev.tokens > 0 ? `${ev.tokens} tokens` : null,
    ]
      .filter(Boolean)
      .join(" · ");
    return html`
      <div class="j-evidence">
        ${segments.map((s) => {
          if (s.kind === "think") {
            return html`<details class="j-ev-fold">
              <summary>思考过程</summary>
              <pre>${s.text}</pre>
            </details>`;
          }
          if (s.kind === "verdict") {
            return html`<details class="j-ev-fold">
              <summary>结构化结论（原始输出）</summary>
              <pre>${s.text}</pre>
            </details>`;
          }
          return html`<p class="j-ev-text">${s.text}</p>`;
        })}
        ${actions.length > 0
          ? html`<div class="j-ev-actions">
              ${actions.map((a) => {
                const r = (a as { result?: Record<string, unknown> }).result;
                const mark = r && "success" in r ? "✓" : r && "failed" in r ? "✗" : "–";
                return html`<div class="j-ev-action">${mark} ${(a as { action_name?: string }).action_name ?? "action"}</div>`;
              })}
            </div>`
          : nothing}
        ${meta ? html`<div class="j-ev-meta">${meta}</div>` : nothing}
      </div>
    `;
  }

  /** 展开区三态：已缓存 → 证据面板；加载中 → 「取证中…」骨架；失败 → 内联重试。 */
  private renderEvidenceArea(ev: BrainEvent): TemplateResult {
    if (this.evidenceFailedIds.has(ev.id)) {
      return html`<div class="j-evidence j-ev-error" @click=${(e: Event) => e.stopPropagation()}>
        证据加载失败，<button class="j-btn-text" @click=${() => this.toggleEvidence(ev)}>重试</button>
      </div>`;
    }
    if (!this.evidenceById.has(ev.id)) {
      return html`<div class="j-evidence j-ev-pending" @click=${(e: Event) => e.stopPropagation()}>取证中…</div>`;
    }
    return html`<div @click=${(e: Event) => e.stopPropagation()}>
      ${this.renderEvidencePanel(this.evidenceById.get(ev.id))}
    </div>`;
  }

  private renderCard(ev: BrainEvent): TemplateResult {
    const needsAction = needsYou(ev);
    const countdown = approvalCountdown(ev);
    const pending = this.pendingIds.has(ev.id);
    const canFeedback = ev.source === "alarm";
    const canEvidence = this.canEvidence(ev);
    const clickable = ev.source === "patrol_tick" || canEvidence;
    return html`
      <div
        class="j-item ${needsAction ? "needs-you" : ""} ${clickable ? "clickable" : ""}"
        @click=${() => this.onRowClick(ev)}
      >
        <div class="j-row1">
          <span class="j-verdict ${needsAction ? "action" : ""}">${BADGE_MAP[ev.status] ?? ev.status}</span>
          <span class="j-title">${ev.title || ev.thingId || "工作区"}</span>
          ${ev.title ? html`<span class="j-device">${ev.thingId ?? "工作区"}</span>` : nothing}
          ${canFeedback
            ? html`<div class="j-fb" @click=${(e: Event) => e.stopPropagation()}>
                <button title="判断正确" ?disabled=${pending} @click=${() => this.vote(ev, "right")}>✓</button>
                <button title="判断错误" ?disabled=${pending} @click=${() => this.vote(ev, "wrong")}>✕</button>
              </div>`
            : nothing}
          <span class="j-time">${fmtJudgmentTime(ev.createdAt)}</span>
        </div>
        <p class="j-reason">${ev.reason || (ev.status === "investigating" ? "AI 正在查证…" : "")}</p>

        ${ev.status === "awaiting_approval"
          ? html`
              <div class="j-actions" @click=${(e: Event) => e.stopPropagation()}>
                ${ev.suggestedAction ? html`<span class="disp-sub">建议：${ev.suggestedAction}</span>` : nothing}
                <button class="btn primary btn-small" ?disabled=${pending} @click=${() => this.approve(ev)}>批准执行</button>
                <button class="btn btn-small" ?disabled=${pending} @click=${() => this.reject(ev)}>拒绝</button>
                ${canEvidence
                  ? html`<button class="j-btn-text" @click=${() => this.toggleEvidence(ev)}>证据</button>`
                  : nothing}
                ${countdown ? html`<span class="j-countdown">${countdown}</span>` : nothing}
              </div>
            `
          : html`
              <div class="j-actions" @click=${(e: Event) => e.stopPropagation()}>
                ${ev.ticketId
                  ? html`<a class="j-btn-text" href="#/tickets/${ev.ticketId}">打开工单 →</a>`
                  : nothing}
                ${canEvidence
                  ? html`<button class="j-btn-text" @click=${() => this.toggleEvidence(ev)}>证据</button>`
                  : nothing}
              </div>
            `}

        ${this.expandedId === ev.id ? this.renderEvidenceArea(ev) : nothing}

        ${this.rejectPanelId === ev.id
          ? html`
              <div class="j-wrong-panel" @click=${(e: Event) => e.stopPropagation()}>
                <textarea
                  placeholder="拒绝原因（必填，会写进工单）"
                  .value=${this.rejectReason}
                  @input=${(e: InputEvent) => (this.rejectReason = (e.target as HTMLTextAreaElement).value)}
                ></textarea>
                <div class="actions">
                  <button class="j-btn-text" @click=${() => (this.rejectPanelId = null)}>取消</button>
                  <button class="btn danger btn-small" ?disabled=${pending} @click=${() => this.submitReject(ev)}>确认拒绝</button>
                </div>
              </div>
            `
          : nothing}

        ${this.wrongPanelId === ev.id
          ? html`
              <div class="j-wrong-panel" @click=${(e: Event) => e.stopPropagation()}>
                <textarea
                  placeholder="哪里不对？这句话会写进 AI 的记忆"
                  .value=${this.wrongReason}
                  @input=${(e: InputEvent) => (this.wrongReason = (e.target as HTMLTextAreaElement).value)}
                ></textarea>
                <div class="actions">
                  <button class="j-btn-text" @click=${() => (this.wrongPanelId = null)}>取消</button>
                  <button class="btn primary btn-small" ?disabled=${pending} @click=${() => this.submitWrong(ev)}>提交</button>
                </div>
              </div>
            `
          : nothing}
      </div>
    `;
  }

  private renderEmpty(): TemplateResult {
    const copy = emptyStateCopy(this.tab, this.summary);
    return html`
      <div class="j-empty">
        <h2>${copy.title}</h2>
        <p>${copy.body}</p>
      </div>
    `;
  }

  render(): TemplateResult {
    return html`
      ${this.renderHeader()}
      ${!this.sseConnected
        ? html`<div class="sse-banner">实时更新已断开，点击刷新获取最新</div>`
        : nothing}
      ${this.renderTabs()}
      ${this.loading
        ? html`<div class="disp-loading">加载中…</div>`
        : this.loadError
          ? html`<div class="disp-error">${this.loadError} <button class="btn btn-small" @click=${() => this.load()}>重试</button></div>`
          : this.events.length === 0
            ? this.renderEmpty()
            : html`
                ${this.events.map((ev) => this.renderCard(ev))}
                ${this.hasMore
                  ? html`<div class="j-more">
                      <button class="btn btn-small" @click=${() => this.loadMore()}>加载更多</button>
                    </div>`
                  : nothing}
              `}
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "view-dispositions": DispositionsView;
  }
}
