/**
 * 工单工作台（工单模块 M1）— 双栏 master-detail。
 *
 * 设计规格（设计文档 Design Review Addendum）：
 * - 布局：≥768px 左列表(320px)+右详情；<768px 列表/详情全宽互斥切换+返回键。
 * - 简报是信任锚点：problem 置顶加粗，steps 编号列表（失败波浪线），
 *   suggested_next_steps 为空时隐藏该节。一律纯文本渲染（禁 unsafeHTML）。
 * - 交互状态：loading 骨架 / 空态「暂无待办工单——Agent 都搞定了」/
 *   错误 toast+重试 / 认领冲突 toast「已被 X 认领」/ SSE 断线细条。
 * - 状态色复用 alarms 语义：待认领 #f97316 / 处理中 #eab308 / 已解决 绿 / 已关闭 灰。
 */

import { LitElement, html, nothing, type TemplateResult } from "lit";
import { customElement, state } from "lit/decorators.js";
import { ticketApi, type Ticket, type TicketDetail, type TicketBriefing } from "../../api/tickets.js";
import { connectSse, type SseConnection } from "../../api/sse-client.js";
import { success, error as toastError, warn } from "../components/toast.js";
import "./tickets.css";

// ── 纯函数助手（vitest 直接测这些，不需要 DOM）──

export type TicketState = Ticket["state"];

export const STATE_TABS = [
  { key: "", label: "全部" },
  { key: "open", label: "待认领" },
  { key: "in_progress", label: "处理中" },
  { key: "resolved", label: "已解决" },
  { key: "closed", label: "已关闭" },
] as const;

export function stateLabel(s: TicketState): string {
  return (
    { open: "待认领", claimed: "已认领", in_progress: "处理中", resolved: "已解决", closed: "已关闭" } as const
  )[s];
}

export function stateBadgeClass(s: TicketState): string {
  return (
    {
      open: "ticket-badge ticket-badge--open",
      claimed: "ticket-badge ticket-badge--claimed",
      in_progress: "ticket-badge ticket-badge--doing",
      resolved: "ticket-badge ticket-badge--done",
      closed: "ticket-badge ticket-badge--closed",
    } as const
  )[s];
}

export function failureKindLabel(kind: string | null | undefined): string {
  if (!kind) return "未知";
  return (
    {
      budget: "预算耗尽",
      policy: "策略拒绝",
      llm: "LLM 失败",
      timeout: "超时",
      tool: "工具执行失败",
      agent_unavailable: "Agent 不可用",
    } as Record<string, string>
  )[kind] ?? kind;
}

/** 各状态下可用的操作（与后端状态机迁移矩阵一致）。 */
export function availableActions(s: TicketState): Array<"claim" | "start" | "resolve" | "close" | "abandon"> {
  switch (s) {
    case "open":
      return ["claim", "close"];
    case "claimed":
      return ["start", "abandon"];
    case "in_progress":
      return ["resolve"];
    case "resolved":
      return ["close"];
    case "closed":
      return [];
  }
}

export function validateResolution(text: string): string | null {
  if (!text.trim()) return "必填：写清楚你实际怎么解决的";
  return null;
}

export function briefingSteps(b: TicketBriefing | undefined) {
  return b?.steps_attempted ?? [];
}

export function hasSuggestions(b: TicketBriefing | undefined): boolean {
  return (b?.suggested_next_steps?.length ?? 0) > 0;
}

// ── 视图 ──

@customElement("view-tickets")
export class TicketsView extends LitElement {
  @state() tickets: Ticket[] = [];
  @state() detail: TicketDetail | null = null;
  @state() loading = true;
  @state() detailLoading = false;
  @state() filterState = "";
  @state() unclaimedCount = 0;
  @state() actionInFlight = false;
  @state() resolutionText = "";
  @state() resolutionError: string | null = null;
  @state() sseConnected = true;
  /** 移动端：false=列表，true=详情 */
  @state() mobileDetail = false;

  private sseConn: SseConnection | null = null;
  private poller: number | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    void this.loadList();
    this.sseConn = connectSse(
      "/api/v1/workspaces/notifications/stream",
      (event) => {
        if (event === "ticket_created") {
          warn("新工单到达：请查看简报");
          void this.loadList();
        }
      },
      () => {
        this.sseConnected = false;
      },
    );
    // 未认领 badge 兜底刷新（SSE 断线时仍可见）。
    this.poller = window.setInterval(() => void this.loadList(true), 60_000);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.sseConn?.close();
    if (this.poller !== null) window.clearInterval(this.poller);
  }

  async loadList(silent = false) {
    if (!silent) this.loading = true;
    try {
      const res = await ticketApi.list({
        state: this.filterState || undefined,
        page: 1,
        page_size: 50,
      });
      this.tickets = res.tickets;
      this.unclaimedCount = res.unclaimed_count;
      this.sseConnected = true;
    } catch (e) {
      toastError(`工单加载失败: ${(e as Error).message}`);
    } finally {
      this.loading = false;
    }
  }

  async openDetail(id: number) {
    this.detailLoading = true;
    this.mobileDetail = true;
    try {
      this.detail = await ticketApi.detail(id);
      this.resolutionText = "";
      this.resolutionError = null;
    } catch (e) {
      toastError(`工单详情加载失败: ${(e as Error).message}`);
    } finally {
      this.detailLoading = false;
    }
  }

  private async act(action: "claim" | "start" | "close" | "abandon") {
    if (!this.detail || this.actionInFlight) return;
    this.actionInFlight = true;
    const id = this.detail.id;
    try {
      await ticketApi[action](id);
      success({ claim: "已认领", start: "已开始处理", close: "已关闭", abandon: "已放弃认领" }[action]);
      await this.openDetail(id);
      void this.loadList(true);
    } catch (e) {
      // 后端 409 文案即冲突语义（「已被 X 认领」等），直接透出。
      toastError((e as Error).message);
      await this.openDetail(id);
      void this.loadList(true);
    } finally {
      this.actionInFlight = false;
    }
  }

  private async submitResolve() {
    if (!this.detail || this.actionInFlight) return;
    const err = validateResolution(this.resolutionText);
    this.resolutionError = err;
    if (err) return;
    this.actionInFlight = true;
    const id = this.detail.id;
    try {
      await ticketApi.resolve(id, this.resolutionText.trim());
      success("已解决，解法已回流 Agent");
      await this.openDetail(id);
      void this.loadList(true);
    } catch (e) {
      toastError((e as Error).message);
    } finally {
      this.actionInFlight = false;
    }
  }

  // ── 渲染 ──

  render() {
    return html`
      <div class="tickets-page">
        <div class="tickets-header">
          <h2>工单</h2>
          ${this.unclaimedCount > 0
            ? html`<span class="unclaimed-badge" aria-label="${this.unclaimedCount} 条待认领工单"
                >${this.unclaimedCount} 待认领</span
              >`
            : nothing}
          <button class="btn btn--ghost" @click=${() => this.loadList()} ?disabled=${this.loading}>刷新</button>
        </div>
        ${!this.sseConnected
          ? html`<div class="sse-banner">实时更新已断开，点击刷新获取最新</div>`
          : nothing}
        <div class="tickets-tabs" role="tablist">
          ${STATE_TABS.map(
            (t) => html`
              <button
                role="tab"
                aria-selected=${this.filterState === t.key}
                class="tab ${this.filterState === t.key ? "tab--active" : ""}"
                @click=${() => {
                  this.filterState = t.key;
                  void this.loadList();
                }}
                >${t.label}</button
              >
            `,
          )}
        </div>
        <div class="tickets-layout ${this.mobileDetail ? "tickets-layout--detail" : ""}">
          <div class="tickets-list">${this.renderList()}</div>
          <div class="tickets-detail">${this.renderDetail()}</div>
        </div>
      </div>
    `;
  }

  private renderList(): TemplateResult {
    if (this.loading) {
      return html`${[1, 2, 3].map(() => html`<div class="ticket-row ticket-row--skeleton"></div>`)}`;
    }
    if (this.tickets.length === 0) {
      return html`
        <div class="tickets-empty">
          <p>暂无待办工单——Agent 都搞定了</p>
          ${this.filterState !== "closed"
            ? html`<button class="btn btn--ghost" @click=${() => {
                this.filterState = "closed";
                void this.loadList();
              }}>查看已关闭</button>`
            : nothing}
        </div>
      `;
    }
    return html`
      ${this.tickets.map(
        (t) => html`
          <div
            class="ticket-row ${this.detail?.id === t.id ? "ticket-row--active" : ""}"
            role="button"
            tabindex="0"
            @click=${() => this.openDetail(t.id)}
            @keydown=${(e: KeyboardEvent) => e.key === "Enter" && this.openDetail(t.id)}
          >
            <div class="ticket-row__title">${t.title}</div>
            <div class="ticket-row__meta">
              <span class=${stateBadgeClass(t.state)}>${stateLabel(t.state)}</span>
              ${t.thing_id ? html`<span class="ticket-row__thing">${t.thing_id}</span>` : nothing}
              <span class="ticket-row__time">${t.created_at.slice(0, 16)}</span>
            </div>
          </div>
        `,
      )}
    `;
  }

  private renderDetail(): TemplateResult {
    if (this.detailLoading) return html`<div class="ticket-panel ticket-panel--skeleton"></div>`;
    const d = this.detail;
    if (!d) {
      return html`<div class="tickets-empty tickets-empty--detail">选择左侧工单查看简报</div>`;
    }
    const actions = availableActions(d.state);
    return html`
      <div class="detail-topbar">
        <button class="btn btn--ghost detail-back" @click=${() => (this.mobileDetail = false)}>← 返回</button>
        <div class="detail-actions">
          ${actions.includes("claim")
            ? html`<button class="btn btn--primary" ?disabled=${this.actionInFlight} @click=${() => this.act("claim")}>认领</button>`
            : nothing}
          ${actions.includes("start")
            ? html`<button class="btn btn--primary" ?disabled=${this.actionInFlight} @click=${() => this.act("start")}>开始处理</button>`
            : nothing}
          ${actions.includes("abandon")
            ? html`<button class="btn btn--ghost" ?disabled=${this.actionInFlight} @click=${() => this.act("abandon")}>放弃认领</button>`
            : nothing}
          ${actions.includes("close")
            ? html`<button class="btn btn--ghost" ?disabled=${this.actionInFlight} @click=${() => this.act("close")}>关闭</button>`
            : nothing}
        </div>
      </div>
      <h3 class="detail-title">${d.title}</h3>
      <div class="detail-meta">
        <span class=${stateBadgeClass(d.state)}>${stateLabel(d.state)}</span>
        ${d.thing_id ? html`<span>物：${d.thing_id}</span>` : html`<span>工作区级故障</span>`}
        ${d.assignee_id ? html`<span>指派人：${d.assignee_id}</span>` : nothing}
        <span>${d.created_at.slice(0, 16)}</span>
      </div>
      ${this.renderBriefing(d.briefing)} ${this.renderTimeline(d)} ${this.renderResolve(d)}
    `;
  }

  private renderBriefing(b: TicketBriefing): TemplateResult {
    const steps = briefingSteps(b);
    return html`
      <section class="ticket-panel briefing">
        <h4 class="ticket-panel__title">Agent 简报 · 我试过了这些</h4>
        <div class="briefing__problem">${b.problem ?? "（无问题描述）"}</div>
        ${steps.length > 0
          ? html`
              <ol class="briefing__steps">
                ${steps.map(
                  (s) => html`
                    <li class=${s.error ? "briefing__step briefing__step--fail" : "briefing__step"}>
                      ${s.action} — ${s.result}${s.error ? html`（${s.error}）` : nothing}
                    </li>
                  `,
                )}
              </ol>
            `
          : nothing}
        <div class="briefing__kv">
          ${b.last_error
            ? html`<div class="briefing__row"><span class="briefing__k">最后错误</span><span>${b.last_error}</span></div>`
            : nothing}
          <div class="briefing__row">
            <span class="briefing__k">失败类型</span>
            <span class="ticket-badge ticket-badge--kind">${failureKindLabel(b.failure_kind)}</span>
          </div>
        </div>
        ${hasSuggestions(b)
          ? html`
              <div class="briefing__suggest">
                <div class="briefing__k">建议</div>
                ${b.suggested_next_steps!.map((s) => html`<p>${s}</p>`)}
              </div>
            `
          : nothing}
      </section>
    `;
  }

  private renderTimeline(d: TicketDetail): TemplateResult {
    if (d.events.length === 0) return html`${nothing}`;
    return html`
      <section class="ticket-panel">
        <h4 class="ticket-panel__title">时间线</h4>
        <ul class="timeline">
          ${d.events.map((e) => {
            const text =
              e.kind === "state_change"
                ? `${e.payload?.from ?? "?"} → ${e.payload?.to ?? "?"}（${e.actor_id ?? e.actor_type}）`
                : e.payload?.kind === "recurrence"
                  ? `故障又触发 ${e.payload.count} 次（最近 run: ${e.payload.agent_run_id}）`
                  : "系统事件";
            return html`<li><span class="timeline__time">${e.created_at.slice(5, 16)}</span>${text}</li>`;
          })}
        </ul>
      </section>
    `;
  }

  private renderResolve(d: TicketDetail): TemplateResult {
    if (d.state === "resolved" && d.resolution_text) {
      return html`
        <section class="ticket-panel">
          <h4 class="ticket-panel__title">解决方案（已回流 Agent 知识）</h4>
          <p class="resolution-text">${d.resolution_text}</p>
        </section>
      `;
    }
    if (d.state !== "in_progress") return html`${nothing}`;
    return html`
      <section class="ticket-panel resolve">
        <h4 class="ticket-panel__title">解决并闭环</h4>
        <label class="resolve__label" for="resolution-input">解决方案（必填）</label>
        <textarea
          id="resolution-input"
          class="resolve__textarea"
          rows="4"
          placeholder="你实际怎么解决的？（例：现场更换轴承 NSK-6205，复位闸阀执行器后备用泵切换正常）"
          .value=${this.resolutionText}
          @input=${(e: InputEvent) => {
            this.resolutionText = (e.target as HTMLTextAreaElement).value;
            this.resolutionError = null;
          }}
        ></textarea>
        ${this.resolutionError ? html`<div class="resolve__error">${this.resolutionError}</div>` : nothing}
        <p class="resolve__hint">这段文字会回流为 Agent 的知识——下次同类故障，Agent 会带着你的解法先自己试。</p>
        <button class="btn btn--primary" ?disabled=${this.actionInFlight} @click=${() => this.submitResolve()}>
          标记已解决
        </button>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    "view-tickets": TicketsView;
  }
}
