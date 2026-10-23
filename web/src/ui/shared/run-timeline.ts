/**
 * run-timeline —— agent run 工具轨迹时间线（AI 运行透明化 T11）。
 *
 * 双模式（mode 属性）：
 * - "live"：卡片内精简行（图标 + 工具中文名 + 参数摘要 + 进行中 spinner）
 * - "full"：详情页（每步 <details> 可展开参数/输出 JSON、✓/✗ 标记、元信息）
 *
 * 步状态：output!=null → done ✓；output==null && live → 进行中 spinner；
 * output==null && !live → 未配对 ToolResult（被取消/流结束）✗。
 *
 * S3 安全裁定：trace 内容（工具名/参数/输出）一律走 lit 文本绑定（自动转义），
 * 禁止 md()/unsafeHTML/innerHTML。
 *
 * 样式令牌：view-scoped --timeline-* 定义在 run-timeline.css 组件根选择器下；
 * 进行中用旋转 spinner，不用脉冲点。
 */
import { LitElement, html, nothing, type TemplateResult } from "lit";
import { customElement, property } from "lit/decorators.js";
import type { TraceEntry } from "../../api/agent-runs.js";
import "./run-timeline.css";

export type RunTimelineMode = "live" | "full";

/** 工具名 → 中文名（以 crates/agent/src/tools/catalog.rs `tool_label` 为准；
 *  thing-agent 三主工具措辞按简报逐字）。未知工具回退原始名。 */
const TOOL_LABELS: Record<string, string> = {
  read_property: "读取设备属性",
  query_events: "查询历史事件",
  invoke_action: "执行处置动作",
  list_things: "列出设备",
  get_thing: "查看设备",
  get_thing_profile: "设备完整快照",
  get_thing_tree: "设备层级树",
  search_knowledge: "搜索知识文档",
  read_document: "读取文档内容",
  alarm_list: "查询告警列表",
  alarm_acknowledge: "确认告警",
};

export function toolLabel(name: string): string {
  return TOOL_LABELS[name] ?? name;
}

/** 参数摘要（live 精简行）：JSON 单行，超长截断。 */
function summarizeArgs(args: unknown): string {
  let s: string;
  try {
    s = JSON.stringify(args) ?? "";
  } catch {
    s = String(args);
  }
  const MAX = 80;
  return s.length > MAX ? `${s.slice(0, MAX)}…` : s;
}

/** 参数/输出完整 JSON（full 模式展开区）。 */
function prettyJson(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2) ?? String(value);
  } catch {
    return String(value);
  }
}

type StepState = "done" | "active" | "failed";

function stepState(e: TraceEntry, live: boolean): StepState {
  if (e.output !== null) return "done";
  return live ? "active" : "failed";
}

@customElement("run-timeline")
export class RunTimeline extends LitElement {
  @property({ type: Array }) entries: TraceEntry[] = [];
  @property({ type: Boolean }) live = false;
  @property({ type: Number }) budgetMax = 0;
  @property({ type: String }) mode: RunTimelineMode = "live";

  /** Light DOM：run-timeline.css 与全局令牌（--ok/--accent/--space-*）直接生效
   * （与 theme-toggle/dispositions 一致）。 */
  createRenderRoot() {
    return this;
  }

  private renderMark(state: StepState): TemplateResult {
    if (state === "done") {
      return html`<span class="rt-mark rt-mark-done" aria-label="已完成">✓</span>`;
    }
    if (state === "active") {
      // 进行中 = 旋转 spinner（评审裁定：禁脉冲点）
      return html`<span class="rt-spinner" role="status" aria-label="进行中"></span>`;
    }
    return html`<span class="rt-mark rt-mark-failed" aria-label="未完成">✗</span>`;
  }

  private renderLiveStep(e: TraceEntry): TemplateResult {
    const state = stepState(e, this.live);
    return html`
      <li class="rt-step rt-step--${state}">
        ${this.renderMark(state)}
        <span class="rt-name">${toolLabel(e.name)}</span>
        <span class="rt-args">${summarizeArgs(e.args)}</span>
      </li>
    `;
  }

  private renderFullStep(e: TraceEntry, index: number): TemplateResult {
    const state = stepState(e, this.live);
    return html`
      <details class="rt-step rt-step--${state}">
        <summary class="rt-step-head">
          ${this.renderMark(state)}
          <span class="rt-idx">#${index + 1}</span>
          <span class="rt-name">${toolLabel(e.name)}</span>
          ${state === "active" ? html`<span class="rt-hint">进行中</span>` : nothing}
        </summary>
        <div class="rt-step-body">
          <div class="rt-field">
            <span class="rt-field-label">参数</span>
            <pre class="rt-json">${prettyJson(e.args)}</pre>
          </div>
          <div class="rt-field">
            <span class="rt-field-label">输出</span>
            ${e.output === null
              ? html`<span class="rt-output-none">${this.live ? "等待结果…" : "无输出（未配对结果）"}</span>`
              : html`<pre class="rt-json">${e.output}</pre>`}
          </div>
          <div class="rt-meta">工具 ${e.name} · 调用 ${e.id}</div>
        </div>
      </details>
    `;
  }

  render() {
    const n = this.entries.length;
    return html`
      <div class="rt-head">
        <span class="rt-title">工具轨迹</span>
        ${this.budgetMax > 0
          ? html`<span class="rt-budget" aria-label="工具调用预算">${n} / ${this.budgetMax}</span>`
          : nothing}
      </div>
      ${n === 0
        ? html`<div class="rt-empty">暂无工具调用</div>`
        : html`
            <ol class="rt-list rt-list--${this.mode}">
              ${this.entries.map((e, i) =>
                this.mode === "full" ? this.renderFullStep(e, i) : this.renderLiveStep(e),
              )}
            </ol>
          `}
    `;
  }
}

/** 供父视图以模板函数形式嵌入（props 绑定，非属性字符串）。 */
export function renderRunTimeline(opts: {
  entries: TraceEntry[];
  live: boolean;
  budgetMax: number;
  mode: RunTimelineMode;
}): TemplateResult {
  return html`<run-timeline
    .entries=${opts.entries}
    .live=${opts.live}
    .budgetMax=${opts.budgetMax}
    .mode=${opts.mode}
  ></run-timeline>`;
}

declare global {
  interface HTMLElementTagNameMap {
    "run-timeline": RunTimeline;
  }
}
