/**
 * 处置中心（AI 大脑主干化 P0）— AI 处置判断 feed。
 *
 * 设计规格（design-review v6 线框稿定稿）：
 * - 单色纪律：颜色只出现在「需要行动」处（琥珀 ● + 白底主按钮），无渐变发光。
 * - 每个判断 = 独立卡片，两行结构（标题行 + 理由行；需行动的理由不截断）。
 * - tab 默认「需要你」（= 待审批 + 需人工）；「全部」留给审计回看。
 * - 动作审批（批准/拒绝）在卡片动作区；判断反馈（✓/✕）固定右上——
 *   审批只影响处置流不写 feedback（F14）；拒绝必填原因；点错必填原因。
 * - 空态 = 产品故事时刻：「一切正常」一句话 + 摘要入口。
 * - SSE：judgment_judged/judgment_updated → 刷新列表与摘要。
 */

import { LitElement, html, nothing, type TemplateResult } from "lit";
import { customElement, state } from "lit/decorators.js";
import {
  judgmentApi,
  type Judgment,
  type JudgmentSummary,
  type JudgmentTab,
} from "../../api/judgments.js";
import { connectSse, type SseConnection } from "../../api/sse-client.js";
import { success, error as toastError } from "../components/toast.js";
import "./dispositions.css";

const TAB_LABELS: Record<JudgmentTab, string> = {
  needs_you: "需要你",
  investigating: "调查中",
  resolved: "已处置",
  noise: "噪声",
  all: "全部",
};

const VERDICT_LABELS: Record<string, string> = {
  noise: "噪声",
  self_healable: "可自愈",
  needs_human: "需人工",
};

const STATUS_LABELS: Record<Judgment["status"], string> = {
  investigating: "调查中…",
  noise_archived: "已静默归档",
  awaiting_approval: "待审批",
  executing: "执行中…",
  resolved: "已处置",
  escalated: "需人工",
  investigation_failed: "调查失败",
  budget_skipped: "未调查（预算）",
};

/** 判断展示标签（导出供测试）。 */
export function verdictLabel(j: Judgment): string {
  if (j.status === "investigating") return "调查中";
  if (j.status === "budget_skipped") return "未调查";
  if (j.verdict) return VERDICT_LABELS[j.verdict] ?? j.verdict;
  return STATUS_LABELS[j.status];
}

/** 「需要你」= 待审批 + 需人工（F3/F15①，导出供测试）。 */
export function needsYou(j: Judgment): boolean {
  return j.status === "awaiting_approval" || j.status === "escalated";
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

/** 审批倒计时文案（F6，导出供测试）：非待审批/无 judgedAt → null。 */
export function approvalCountdown(j: Judgment, now = Date.now()): string | null {
  if (j.status !== "awaiting_approval" || !j.judgedAt) return null;
  const deadline = new Date(j.judgedAt).getTime() + 24 * 3600_000;
  const remainH = Math.max(0, Math.round((deadline - now) / 3600_000));
  return `${remainH} 小时后自动转工单`;
}

/** 反馈校验（F13，导出供测试）：点错必填 ≥4 字符。 */
export function validateWrongReason(reason: string): string | null {
  return reason.trim().length >= 4 ? null : "点错必须填写原因（至少 4 个字符）";
}

@customElement("view-dispositions")
export class DispositionsView extends LitElement {
  @state() private judgments: Judgment[] = [];
  @state() private summary: JudgmentSummary | null = null;
  @state() private tab: JudgmentTab = "needs_you";
  @state() private loading = true;
  @state() private loadError: string | null = null;
  @state() private expandedId: string | null = null;
  @state() private wrongPanelId: string | null = null;
  @state() private wrongReason = "";
  @state() private rejectPanelId: string | null = null;
  @state() private rejectReason = "";

  private sseConn: SseConnection | null = null;
  private poller: number | null = null;

  connectedCallback() {
    super.connectedCallback();
    void this.load();
    this.sseConn = connectSse("/api/v1/workspaces/notifications/stream", (event) => {
      if (event === "judgment_judged" || event === "judgment_updated" || event === "ticket_created") {
        void this.load(true);
      }
    });
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
      const [judgments, summary] = await Promise.all([
        judgmentApi.list(this.tab, undefined, 50),
        judgmentApi.summary(),
      ]);
      this.judgments = judgments;
      this.summary = summary;
    } catch (e) {
      this.loadError = e instanceof Error ? e.message : "加载失败";
    } finally {
      this.loading = false;
    }
  }

  private switchTab(tab: JudgmentTab) {
    if (tab === this.tab) return;
    this.tab = tab;
    this.expandedId = null;
    this.wrongPanelId = null;
    this.rejectPanelId = null;
    void this.load();
  }

  private async vote(j: Judgment, verdict: "right" | "wrong") {
    if (verdict === "wrong") {
      this.wrongPanelId = this.wrongPanelId === j.id ? null : j.id;
      this.wrongReason = "";
      return;
    }
    try {
      await judgmentApi.feedback(j.id, "right");
      success("已记录：AI 判断正确");
      void this.load(true);
    } catch (e) {
      toastError(e instanceof Error ? e.message : "反馈失败");
    }
  }

  private async submitWrong(j: Judgment) {
    const err = validateWrongReason(this.wrongReason);
    if (err) {
      toastError(err);
      return;
    }
    try {
      await judgmentApi.feedback(j.id, "wrong", this.wrongReason.trim());
      success("已写入 AI 的记忆，下次它会做得更好");
      this.wrongPanelId = null;
      void this.load(true);
    } catch (e) {
      toastError(e instanceof Error ? e.message : "反馈失败");
    }
  }

  private async approve(j: Judgment) {
    try {
      await judgmentApi.approve(j.id);
      success("已批准，开始执行");
      void this.load(true);
    } catch (e) {
      toastError(e instanceof Error ? e.message : "批准失败");
    }
  }

  private async submitReject(j: Judgment) {
    const reason = this.rejectReason.trim();
    if (reason.length < 4) {
      toastError("拒绝必须填写原因（至少 4 个字符）");
      return;
    }
    try {
      await judgmentApi.reject(j.id, reason);
      success("已拒绝并转工单");
      this.rejectPanelId = null;
      void this.load(true);
    } catch (e) {
      toastError(e instanceof Error ? e.message : "拒绝失败");
    }
  }


  private renderHeader(): TemplateResult {
    const s = this.summary;
    return html`
      <div class="disp-head">
        <h1 class="page-title">处置中心</h1>
        <span class="disp-sub">
          ${s ? html`今日 ${s.digestedToday} 条已消化 · ${s.needsYou} 条需要你` : "…"}
        </span>
      </div>
      ${s && s.feedbackTotal > 0
        ? html`<p class="disp-learn">AI 已从你的 ${s.feedbackTotal} 条反馈中学习
            （${s.feedbackRight} 对 / ${s.feedbackWrong} 错）</p>`
        : nothing}
    `;
  }

  private renderTabs(): TemplateResult {
    return html`
      <div class="disp-tabs">
        ${(Object.keys(TAB_LABELS) as JudgmentTab[]).map(
          (t) => html`
            <button class="disp-tab ${t === this.tab ? "active" : ""}" @click=${() => this.switchTab(t)}>
              ${TAB_LABELS[t]}
            </button>
          `,
        )}
      </div>
    `;
  }

  private renderCard(j: Judgment): TemplateResult {
    const needsAction = needsYou(j);
    const countdown = approvalCountdown(j);
    const voted = j.latestFeedback?.verdict;
    return html`
      <div class="j-item ${needsAction ? "needs-you" : ""}">
        <div class="j-row1">
          <span class="j-verdict ${needsAction ? "action" : ""}">${verdictLabel(j)}</span>
          <span class="j-device">${j.thingId ?? "工作区"} · ${STATUS_LABELS[j.status]}</span>
          <div class="j-fb">
            <button
              class="${voted === "right" ? "voted" : ""}"
              title="判断正确"
              @click=${() => this.vote(j, "right")}
            >✓</button>
            <button
              class="${voted === "wrong" ? "voted" : ""}"
              title="判断错误"
              @click=${() => this.vote(j, "wrong")}
            >✕</button>
          </div>
          <span class="j-time">${fmtJudgmentTime(j.createdAt)}</span>
        </div>
        <p class="j-reason">${j.reason || "AI 正在查证…"}</p>

        ${j.status === "awaiting_approval"
          ? html`
              <div class="j-actions">
                ${j.suggestedAction ? html`<span class="disp-sub">建议：${j.suggestedAction}</span>` : nothing}
                <button class="j-btn primary" @click=${() => this.approve(j)}>批准执行</button>
                <button
                  class="j-btn"
                  @click=${() => {
                    this.rejectPanelId = this.rejectPanelId === j.id ? null : j.id;
                    this.rejectReason = "";
                  }}
                >拒绝</button>
                <button
                  class="j-btn text"
                  @click=${() => (this.expandedId = this.expandedId === j.id ? null : j.id)}
                >证据</button>
                ${countdown ? html`<span class="j-countdown">${countdown}</span>` : nothing}
              </div>
            `
          : html`
              <div class="j-actions">
                ${j.ticketId
                  ? html`<a class="j-btn text" href="#/tickets/${j.ticketId}">打开工单 →</a>`
                  : nothing}
                ${j.evidence && j.status !== "investigating"
                  ? html`
                      <button
                        class="j-btn text"
                        @click=${() => (this.expandedId = this.expandedId === j.id ? null : j.id)}
                      >证据</button>
                    `
                  : nothing}
              </div>
            `}

        ${this.expandedId === j.id
          ? html`<div class="j-evidence">${JSON.stringify(j.evidence, null, 2)}</div>`
          : nothing}

        ${this.rejectPanelId === j.id
          ? html`
              <div class="j-wrong-panel">
                <textarea
                  placeholder="拒绝原因（必填，会写进工单）"
                  .value=${this.rejectReason}
                  @input=${(e: InputEvent) => (this.rejectReason = (e.target as HTMLTextAreaElement).value)}
                ></textarea>
                <div class="actions">
                  <button class="j-btn text" @click=${() => (this.rejectPanelId = null)}>取消</button>
                  <button class="j-btn primary" @click=${() => this.submitReject(j)}>确认拒绝</button>
                </div>
              </div>
            `
          : nothing}

        ${this.wrongPanelId === j.id
          ? html`
              <div class="j-wrong-panel">
                <textarea
                  placeholder="哪里不对？这句话会写进 AI 的记忆"
                  .value=${this.wrongReason}
                  @input=${(e: InputEvent) => (this.wrongReason = (e.target as HTMLTextAreaElement).value)}
                ></textarea>
                <div class="actions">
                  <button class="j-btn text" @click=${() => (this.wrongPanelId = null)}>取消</button>
                  <button class="j-btn primary" @click=${() => this.submitWrong(j)}>提交</button>
                </div>
              </div>
            `
          : nothing}
      </div>
    `;
  }

  private renderEmpty(): TemplateResult {
    return html`
      <div class="j-empty">
        <h2>一切正常</h2>
        <p>
          ${this.summary
            ? html`今日 ${this.summary.digestedToday} 条报警已消化，没有需要你处理的事。`
            : "没有需要你处理的事。"}
        </p>
      </div>
    `;
  }

  render(): TemplateResult {
    return html`
      ${this.renderHeader()} ${this.renderTabs()}
      ${this.loading
        ? html`<div class="disp-loading">加载中…</div>`
        : this.loadError
          ? html`<div class="disp-error">${this.loadError} <button class="j-btn" @click=${() => this.load()}>重试</button></div>`
          : this.judgments.length === 0
            ? this.renderEmpty()
            : this.judgments.map((j) => this.renderCard(j))}
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "view-dispositions": DispositionsView;
  }
}
