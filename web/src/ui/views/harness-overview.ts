import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { getHarnessOverview, listHarnessSubagents } from '../../api/harness.js';
import type {
  HarnessAttentionItem,
  HarnessMainTaskSummary,
  HarnessOverview,
  HarnessSubagentSummary,
  HarnessWorkflowSummary,
} from '../../types/harness.js';
import { toHarnessTaskUrl, type HarnessRoute } from './harness-url-state.js';
import './harness-overview.css';

const attentionPageSize = 5;
const subagentPageSize = 20;

const statusLabels: Record<string, string> = {
  active: '进行中',
  running: '进行中',
  waiting_job: '等待设备',
  waiting_watch: '等待观察',
  needs_approval: '等待审批',
  paused: '已暂停',
  cancelling: '取消中',
  completed: '已完成',
  failed: '失败',
  cancelled: '已取消',
  scheduled: '计划',
  pending: '等待调度',
  unknown: '结果未知',
  cancelled_unknown: '停止结果未知',
  simulation: '模拟运行',
};

const statusTones: Record<string, string> = {
  active: 'accent',
  running: 'accent',
  waiting_job: 'accent',
  waiting_watch: 'accent',
  needs_approval: 'warn',
  paused: 'muted',
  cancelling: 'warn',
  completed: 'ok',
  failed: 'danger',
  cancelled: 'muted',
  scheduled: 'accent',
  pending: 'warn',
  unknown: 'danger',
  cancelled_unknown: 'danger',
  simulation: 'muted',
};

const attentionKindLabels: Record<string, string> = {
  effect_unknown: '结果未知',
  recovery: '需要恢复',
  cancelled_unknown: '停止结果未知',
  approval: '等待审批',
};

function statusLabel(status: string): string {
  return statusLabels[status] ?? '未知状态';
}

function statusTone(status: string): string {
  return statusTones[status] ?? 'muted';
}

function attentionKindLabel(kind: string): string {
  return attentionKindLabels[kind] ?? '需要处理';
}

function timestamp(value: string | null): string {
  return value ?? '暂无';
}

function mainTaskIdFromAnchor(anchor: string | null): string | null {
  if (!anchor?.startsWith('main-')) return null;
  return anchor.slice('main-'.length) || null;
}

function attentionOffsetFromAnchor(anchor: string | null): number | null {
  if (!anchor) return null;
  if (anchor === 'harness-attention') return 0;
  const match = anchor.match(/^harness-attention-(\d+)$/);
  return match ? Number(match[1]) : null;
}

function anchorTargetId(anchor: string): string {
  return anchor === 'harness-attention' || anchor.startsWith('harness-attention-')
    ? 'harness-attention'
    : anchor;
}

@customElement('view-harness-overview')
export class HarnessOverviewView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';
  @property({ attribute: false }) route: HarnessRoute | null = null;
  @property({ attribute: false }) anchor: string | null = null;

  @state() private overview: HarnessOverview | null = null;
  @state() private connection: 'connecting' | 'online' | 'offline' = 'connecting';
  @state() private error = '';
  @state() private attentionOffset = 0;
  @state() private attentionCollapsed = false;
  @state() private expandedMains: string[] = [];
  @state() private childItems: Record<string, HarnessSubagentSummary[]> = {};
  @state() private childTotals: Record<string, number> = {};
  @state() private childLoading: string[] = [];
  @state() private childErrors: Record<string, string> = {};
  @state() private announcement = '';

  private generation = 0;
  private abort: AbortController | null = null;
  private childAborts = new Map<string, AbortController>();
  private loadedWorkspace = '';
  private pendingAnchor: string | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    const anchoredOffset = attentionOffsetFromAnchor(this.anchor);
    if (anchoredOffset !== null) this.attentionOffset = anchoredOffset;
    void this.load(this.attentionOffset);
  }

  disconnectedCallback(): void {
    this.generation += 1;
    this.abort?.abort();
    for (const controller of this.childAborts.values()) controller.abort();
    this.childAborts.clear();
    super.disconnectedCallback();
  }

  protected updated(): void {
    if (!this.pendingAnchor) return;
    const anchor = this.pendingAnchor;
    this.pendingAnchor = null;
    const target = document.getElementById(anchorTargetId(anchor));
    if (target && typeof target.scrollIntoView === 'function') {
      target.scrollIntoView({ block: 'start' });
    }
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (!changed.has('workspaceId')) return;
    const previous = changed.get('workspaceId');
    if (previous !== undefined && previous !== '' && this.workspaceId !== this.loadedWorkspace) {
      this.attentionOffset = 0;
      void this.load(0);
    }
  }

  private async load(offset: number): Promise<void> {
    if (!this.workspaceId) {
      this.connection = 'offline';
      this.error = '未选择工作空间';
      return;
    }
    const generation = ++this.generation;
    this.abort?.abort();
    this.abort = new AbortController();
    this.connection = 'connecting';
    this.error = '';
    try {
      const response = await getHarnessOverview(
        this.workspaceId,
        { attentionLimit: attentionPageSize, attentionOffset: offset },
        this.abort.signal,
      );
      if (generation !== this.generation) return;
      const result = response.result;
      if (!result) throw new Error('空响应');
      this.overview = result;
      const sameWorkspace = this.loadedWorkspace === this.workspaceId;
      this.loadedWorkspace = this.workspaceId;
      this.attentionOffset = result.attention.offset;
      if (!sameWorkspace) {
        for (const controller of this.childAborts.values()) controller.abort();
        this.childAborts.clear();
        this.childErrors = {};
        this.expandedMains = [];
      }
      const anchoredMain = mainTaskIdFromAnchor(this.anchor);
      if (anchoredMain && !this.expandedMains.includes(anchoredMain)) {
        this.expandedMains = [...this.expandedMains, anchoredMain];
      }
      if (this.anchor) this.pendingAnchor = this.anchor;
      const children: Record<string, HarnessSubagentSummary[]> = {};
      const totals: Record<string, number> = {};
      for (const main of result.coordination) {
        // Attention paging refreshes the aggregate; keep already loaded child
        // pages instead of throwing them away on every page turn.
        const existing = sameWorkspace ? (this.childItems[main.taskId] ?? []) : [];
        const seen = new Set(existing.map((child) => child.taskId));
        const merged = [...existing];
        for (const child of main.subagents) {
          if (seen.has(child.taskId)) continue;
          seen.add(child.taskId);
          merged.push(child);
        }
        children[main.taskId] = merged;
        totals[main.taskId] = Math.max(main.subagentTotal, merged.length);
      }
      this.childItems = children;
      this.childTotals = totals;
      this.connection = 'online';
    } catch (error) {
      if (generation !== this.generation || this.abort.signal.aborted) return;
      this.connection = 'offline';
      this.error = error instanceof ApiError ? error.message : '加载 Harness 概览失败';
    }
  }

  private retry(): void {
    void this.load(this.attentionOffset);
  }

  private page(offset: number): void {
    const next = Math.max(0, offset);
    this.attentionOffset = next;
    void this.load(next);
  }

  private toggleAttention(): void {
    this.attentionCollapsed = !this.attentionCollapsed;
    this.announcement = this.attentionCollapsed ? '已收起需要处理的事项' : '已展开需要处理的事项';
  }

  private toggleMain(taskId: string): void {
    const expanded = this.expandedMains.includes(taskId);
    this.expandedMains = expanded
      ? this.expandedMains.filter((id) => id !== taskId)
      : [...this.expandedMains, taskId];
    this.announcement = expanded
      ? `已收起 Main ${taskId} 的 Subagent`
      : `已展开 Main ${taskId} 的 Subagent`;
  }

  private async loadMoreChildren(main: HarnessMainTaskSummary): Promise<void> {
    const existing = this.childItems[main.taskId] ?? [];
    const generation = this.generation;
    this.childAborts.get(main.taskId)?.abort();
    const controller = new AbortController();
    this.childAborts.set(main.taskId, controller);
    this.childLoading = [...new Set([...this.childLoading, main.taskId])];
    this.childErrors = { ...this.childErrors, [main.taskId]: '' };
    try {
      const response = await listHarnessSubagents(
        this.workspaceId,
        main.taskId,
        subagentPageSize,
        existing.length,
        controller.signal,
      );
      if (generation !== this.generation || controller.signal.aborted) return;
      const page = response.result;
      if (!page) throw new Error('空响应');
      this.childItems = {
        ...this.childItems,
        [main.taskId]: [...existing, ...page.subagents],
      };
      this.childTotals = {
        ...this.childTotals,
        [main.taskId]: page.total,
      };
      this.announcement = `已加载 ${page.subagents.length} 个 Subagent`;
    } catch (error) {
      if (generation !== this.generation || controller.signal.aborted) return;
      this.childErrors = {
        ...this.childErrors,
        [main.taskId]: error instanceof ApiError ? error.message : '加载 Subagent 失败',
      };
    } finally {
      if (this.childAborts.get(main.taskId) === controller) {
        this.childAborts.delete(main.taskId);
        this.childLoading = this.childLoading.filter((id) => id !== main.taskId);
      }
    }
  }

  private renderAttentionItem(item: HarnessAttentionItem, offset: number) {
    const href = item.taskId
      ? toHarnessTaskUrl(item.taskId, {
          event: item.eventId,
          from: `#/harness#harness-attention-${offset}`,
        })
      : null;
    return html`
      <li class="harness-attention__item" data-attention-id=${item.id}>
        <div class="harness-attention__title">
          <span class="harness-attention__kind">${attentionKindLabel(item.kind)}</span>
          <strong>${item.title}</strong>
        </div>
        <p class="harness-attention__detail">${item.detail}</p>
        <div class="harness-attention__meta">
          <time datetime=${item.occurredAt}>${item.occurredAt}</time>
          ${
            href
              ? html`<a class="harness-attention__action" href=${href}
                  >${item.actionLabel || '查看并处理'}</a
                >`
              : nothing
          }
        </div>
      </li>
    `;
  }

  private renderAttention() {
    const page = this.overview?.attention;
    if (!page) return nothing;
    const start = page.total === 0 ? 0 : page.offset + 1;
    const end = Math.min(page.offset + page.items.length, page.total);
    const limit = page.limit > 0 ? page.limit : attentionPageSize;
    return html`
      <section
        class="harness-section harness-attention"
        data-harness-section="attention"
        aria-labelledby="harness-attention-title"
        id="harness-attention"
      >
        <div class="harness-section__header">
          <div>
            <h2 id="harness-attention-title">需要处理</h2>
            <p class="harness-section__summary">
              ${page.total === 0 ? '当前没有需要处理的事项' : `共 ${page.total} 项，先处理例外再查看配置。`}
            </p>
          </div>
          <button
            type="button"
            class="btn harness-attention__toggle"
            aria-expanded=${this.attentionCollapsed ? 'false' : 'true'}
            aria-controls="harness-attention-list"
            @click=${() => this.toggleAttention()}
          >
            ${this.attentionCollapsed ? '展开' : '收起'}
          </button>
        </div>
        ${
          page.items.length === 0
            ? html`<p class="harness-empty">当前没有需要处理的事项。</p>`
            : nothing
        }
        <div id="harness-attention-list" ?hidden=${this.attentionCollapsed}>
          ${
            this.attentionCollapsed || page.items.length === 0
              ? nothing
              : html`<ul class="harness-attention__list">
                  ${page.items.map((item) => this.renderAttentionItem(item, page.offset))}
                </ul>`
          }
        </div>
        <div class="harness-section__footer">
          <span class="harness-attention__range">
            ${page.total === 0 ? '第 0 项，共 0 项' : `第 ${start}-${end} 项，共 ${page.total} 项`}
          </span>
          <button
            type="button"
            class="btn harness-attention__previous"
            ?disabled=${page.offset === 0 || this.connection === 'connecting'}
            @click=${() => this.page(page.offset - limit)}
          >
            上一页
          </button>
          <button
            type="button"
            class="btn harness-attention__next"
            ?disabled=${page.offset + limit >= page.total || this.connection === 'connecting'}
            @click=${() => this.page(page.offset + limit)}
          >
            下一页
          </button>
        </div>
      </section>
    `;
  }

  private renderWorkflow(workflow: HarnessWorkflowSummary) {
    return html`
      <li class="harness-workflow" data-workflow-id=${workflow.workflowId}>
        <div class="harness-workflow__head">
          <strong>工作流 ${workflow.objective}</strong>
          <span class="harness-status harness-status--${workflow.enabled ? 'ok' : 'muted'}"
            >${workflow.enabled ? '已启用' : '未启用'}</span
          >
        </div>
        <div class="harness-workflow__meta">
          <span>触发：${workflow.triggerLabel}</span>
          <span>版本：${workflow.version}</span>
          <span>下一次：${workflow.enabled ? timestamp(workflow.nextRunAt) : '未计划'}</span>
        </div>
        <p class="harness-workflow__result">最近结果：${workflow.lastResult ?? '暂无'}</p>
      </li>
    `;
  }

  private renderWorkflows() {
    const workflows = this.overview?.workflows ?? [];
    return html`
      <section
        class="harness-section harness-workflows"
        data-harness-section="workflows"
        aria-labelledby="harness-workflows-title"
      >
        <div class="harness-section__header">
          <div>
            <h2 id="harness-workflows-title">Workflow 与调度</h2>
            <p class="harness-section__summary">
              已启用 ${workflows.filter((workflow) => workflow.enabled).length} 个，共
              ${workflows.length} 个。
            </p>
          </div>
        </div>
        ${
          workflows.length === 0
            ? html`<p class="harness-empty">
                尚未配置 Workflow。新建后，Harness 会按声明自动调度。
              </p>`
            : html`<ul class="harness-workflow__list">
                ${workflows.map((workflow) => this.renderWorkflow(workflow))}
              </ul>`
        }
      </section>
    `;
  }

  private renderSubagent(main: HarnessMainTaskSummary, child: HarnessSubagentSummary) {
    return html`
      <li class="harness-subagent" data-task-id=${child.taskId}>
        <div class="harness-subagent__head">
          <span class="harness-subagent__id">${child.taskId}</span>
          <span class="harness-status harness-status--${statusTone(child.status)}"
            >${statusLabel(child.status)}</span
          >
        </div>
        <div class="harness-subagent__facts">
          <span>等待原因：${child.waitReason ?? '无'}</span>
          <span>最近结果：${child.recentResult ?? '暂无'}</span>
        </div>
        <a
          class="harness-subagent__task"
          href=${toHarnessTaskUrl(child.taskId, {
            from: `#/harness#main-${main.taskId}`,
          })}
          >打开 Task</a
        >
      </li>
    `;
  }

  private renderChildren(main: HarnessMainTaskSummary) {
    const children = this.childItems[main.taskId] ?? [];
    const total = this.childTotals[main.taskId] ?? main.subagentTotal;
    const loading = this.childLoading.includes(main.taskId);
    const error = this.childErrors[main.taskId];
    return html`
      <div class="harness-children">
        <p class="harness-children__count">已显示 ${children.length} / 共 ${total}</p>
        ${
          children.length === 0
            ? html`<p class="harness-empty">暂无 Subagent。</p>`
            : html`<ul class="harness-subagent__list">
                ${children.map((child) => this.renderSubagent(main, child))}
              </ul>`
        }
        ${error ? html`<p class="harness-children__error" role="alert">${error}</p>` : nothing}
        ${
          children.length < total
            ? html`<button
                type="button"
                class="btn harness-children__more"
                ?disabled=${loading}
                @click=${() => this.loadMoreChildren(main)}
              >
                ${loading ? '加载中…' : '加载更多 Subagent'}
              </button>`
            : nothing
        }
      </div>
    `;
  }

  private renderMain(main: HarnessMainTaskSummary) {
    const expanded = this.expandedMains.includes(main.taskId);
    return html`
      <li class="harness-main" id="main-${main.taskId}" data-task-id=${main.taskId}>
        <div class="harness-main__head">
          <button
            type="button"
            class="harness-main__toggle"
            aria-expanded=${expanded ? 'true' : 'false'}
            aria-controls="subagents-${main.taskId}"
            @click=${() => this.toggleMain(main.taskId)}
          >
            <span class="harness-main__chevron" aria-hidden="true">${expanded ? '▾' : '▸'}</span>
            <span class="harness-main__objective">${main.objective}</span>
          </button>
          <span class="harness-status harness-status--${statusTone(main.status)}"
            >${statusLabel(main.status)}</span
          >
        </div>
        <div class="harness-main__facts">
          <span>等待原因：${main.waitReason ?? '无'}</span>
          <span>最近结果：${main.recentResult ?? '暂无'}</span>
        </div>
        <div class="harness-main__actions">
          <a class="harness-main__task" href=${toHarnessTaskUrl(main.taskId)}>打开 Main Task</a>
        </div>
        <div id="subagents-${main.taskId}" ?hidden=${!expanded}>
          ${expanded ? this.renderChildren(main) : nothing}
        </div>
      </li>
    `;
  }

  private renderCoordination() {
    const mains = this.overview?.coordination ?? [];
    return html`
      <section
        class="harness-section harness-coordination"
        data-harness-section="coordination"
        aria-labelledby="harness-coordination-title"
      >
        <div class="harness-section__header">
          <div>
            <h2 id="harness-coordination-title">当前 Main / Subagent</h2>
            <p class="harness-section__summary">
              展开 Main 查看隔离的 Subagent；Subagent 使用同一 Task 详情与写路径。
            </p>
          </div>
        </div>
        ${
          mains.length === 0
            ? html`<p class="harness-empty">当前没有 Main 任务。</p>`
            : html`<ul class="harness-main__list">
                ${mains.map((main) => this.renderMain(main))}
              </ul>`
        }
      </section>
    `;
  }

  private renderStatus() {
    if (this.connection === 'online') return nothing;
    if (this.connection === 'connecting') {
      return html`<p class="harness-overview__status" role="status">连接中…</p>`;
    }
    return html`<p class="harness-overview__status" role="status">
      ${this.overview ? '离线，显示上次结果。' : '无法加载，请重试。'}
    </p>`;
  }

  render() {
    const workflowPlaceholder = this.route?.name === 'workflows';
    return html`
      <section class="harness-overview" aria-labelledby="harness-overview-title">
        <header class="harness-overview__header">
          <div class="harness-overview__intro">
            <h1 id="harness-overview-title">Harness 工作台</h1>
            <p>例外优先：先处理需要你确认的事项，再查看计划任务与当前协调。</p>
          </div>
          <div class="harness-overview__actions">
            <button type="button" class="btn harness-overview__retry" @click=${() => this.retry()}>
              刷新
            </button>
            ${
              workflowPlaceholder
                ? html`<button
                    type="button"
                    class="btn btn--primary harness-overview__primary"
                    disabled
                    aria-describedby="harness-workflow-placeholder"
                  >
                    新建工作流（下一步提供）
                  </button>`
                : html`<a
                    class="btn btn--primary harness-overview__primary"
                    href="#/harness/workflows/new"
                    >新建工作流</a
                  >`
            }
          </div>
        </header>
        ${
          workflowPlaceholder
            ? html`<p id="harness-workflow-placeholder" class="harness-overview__notice">
                工作流编辑器将在下一步提供；当前可查看已有 Workflow 与调度状态。
              </p>`
            : nothing
        }
        ${this.renderStatus()}
        ${
          this.error
            ? html`<p class="harness-overview__error" role="alert">
                加载 Harness 概览失败：${this.error}
              </p>`
            : nothing
        }
        <p class="harness-sr-only" role="status" aria-live="polite">${this.announcement}</p>
        ${
          this.overview
            ? html`<div class="harness-overview__sections">
                ${this.renderAttention()} ${this.renderWorkflows()} ${this.renderCoordination()}
              </div>`
            : nothing
        }
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-harness-overview': HarnessOverviewView;
  }
}
