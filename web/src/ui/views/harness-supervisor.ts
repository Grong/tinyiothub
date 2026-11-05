import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { getHarnessSupervisor, retryHarnessDeadWake } from '../../api/harness.js';
import type {
  HarnessDeadWake,
  HarnessSupervisor,
  HarnessSupervisorHealth,
} from '../../types/harness.js';
import { formatHarnessTime, nextAnnouncement, renderHarnessStatus } from './harness-status.js';
import './harness-supervisor.css';

interface InlineResult {
  kind: 'success' | 'error';
  message: string;
}

/** Outbox identity is `(task_id, event_id)`; event IDs alone may collide. */
function wakeKey(row: Pick<HarnessDeadWake, 'taskId' | 'eventId'>): string {
  return `${row.taskId}\u0000${row.eventId}`;
}

@customElement('view-harness-supervisor')
export class HarnessSupervisorView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';

  @state() private data: HarnessSupervisor | null = null;
  @state() private error = '';
  @state() private loading = false;
  @state() private retrying: Record<string, boolean> = {};
  @state() private results: Record<string, InlineResult> = {};
  @state() private announcement = '';

  private generation = 0;
  private abort: AbortController | null = null;
  private loadedWorkspace = '';

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    void this.load();
  }

  disconnectedCallback(): void {
    this.generation += 1;
    this.abort?.abort();
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (!changed.has('workspaceId')) return;
    const previous = changed.get('workspaceId');
    if (previous !== undefined && previous !== '' && this.workspaceId !== this.loadedWorkspace) {
      this.retrying = {};
      this.results = {};
      void this.load();
    }
  }

  private async load(): Promise<void> {
    if (!this.workspaceId) {
      this.error = '未选择工作空间';
      return;
    }
    const generation = ++this.generation;
    this.abort?.abort();
    this.abort = new AbortController();
    this.loading = true;
    this.error = '';
    try {
      const response = await getHarnessSupervisor(this.workspaceId, this.abort.signal);
      if (generation !== this.generation) return;
      const result = response.result;
      if (!result) throw new Error('空响应');
      this.data = result;
      this.loadedWorkspace = this.workspaceId;
    } catch (error) {
      if (generation !== this.generation || this.abort.signal.aborted) return;
      this.error = error instanceof ApiError ? error.message : '加载 Supervisor 健康失败';
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }

  private retry(row: HarnessDeadWake): void {
    void this.performRetry(row);
  }

  private async performRetry(row: HarnessDeadWake): Promise<void> {
    // The outbox identity is (task_id, event_id); two Tasks may legitimately
    // carry the same event ID, so per-row state must use the composite key.
    const key = wakeKey(row);
    const eventId = row.eventId;
    if (this.retrying[key]) return;
    this.retrying = { ...this.retrying, [key]: true };
    const previous = this.results[key];
    this.results = { ...this.results, [key]: { kind: 'success', message: '' } };
    try {
      await retryHarnessDeadWake(this.workspaceId, row.taskId, eventId);
      // The original row stays in the list: the durable fact is that the
      // same event identity is now pending, not that the row was replaced.
      this.results = {
        ...this.results,
        [key]: {
          kind: 'success',
          message: `已按原事件 ${eventId} 重新排队，状态：等待调度。`,
        },
      };
      this.announcement = nextAnnouncement(this.announcement, `已按原事件 ${eventId} 重新排队`);
    } catch (error) {
      const message = error instanceof ApiError ? error.message : '重试失败';
      this.results = {
        ...this.results,
        [key]:
          previous?.kind === 'error' && previous.message === message
            ? previous
            : {
                kind: 'error',
                message: `${message} 下一页操作：确认权限后重试该事件。`,
              },
      };
    } finally {
      const next = { ...this.retrying };
      delete next[key];
      this.retrying = next;
    }
  }

  private renderHealthFacts(health: HarnessSupervisorHealth) {
    return html`
      <dl class="harness-supervisor__facts" data-health-facts>
        <div>
          <dt>下一次计划唤醒</dt>
          <dd>
            ${formatHarnessTime(health.nextWakeAt)}
            <span class="harness-supervisor__hint">计划时间，不承诺准点</span>
          </dd>
        </div>
        <div>
          <dt>最早已等待</dt>
          <dd>${formatHarnessTime(health.oldestPendingAt)}</dd>
        </div>
        <div>
          <dt>调度延迟</dt>
          <dd>${health.schedulingDelaySeconds} 秒</dd>
        </div>
        <div>
          <dt>活动 Main</dt>
          <dd>${health.activeMains}</dd>
        </div>
        <div>
          <dt>活动 Subagent</dt>
          <dd>${health.activeSubagents}</dd>
        </div>
        <div>
          <dt>最近错误</dt>
          <dd>${health.lastError ?? '暂无'}</dd>
        </div>
        <div>
          <dt>最近完成 tick</dt>
          <dd>${formatHarnessTime(health.lastCompletedAt)} · 序号 ${health.tickSeq}</dd>
        </div>
        <div>
          <dt>已启用 Workflow</dt>
          <dd>${health.enabledWorkflows}</dd>
        </div>
        <div>
          <dt>等待调度 Wake</dt>
          <dd>${health.pendingWakes}</dd>
        </div>
      </dl>
    `;
  }

  private renderHealth() {
    const health = this.data?.health;
    if (!health) return nothing;
    const stalled = health.status === 'stalled';
    return html`
      <section
        class="harness-supervisor__section"
        aria-labelledby="harness-supervisor-health-title"
      >
        <div class="harness-supervisor__section-header">
          <div>
            <h2 id="harness-supervisor-health-title">调度健康</h2>
            <p class="harness-supervisor__summary">
              来自持久 tick 与 Workflow/Wake 事实；AI 心跳不作为进程存活证据。
            </p>
          </div>
          ${renderHarnessStatus(health.status)}
        </div>
        ${
          stalled
            ? html`<p class="harness-supervisor__stalled" role="alert">
                ${health.stalledLabel ?? '调度器停滞'}
              </p>`
            : nothing
        }
        ${
          !stalled && health.noNewRuns
            ? html`<p class="harness-supervisor__quiet">
                当前没有新的 Run；这是正常空闲，不代表调度器停滞。下一次计划唤醒见下方。
              </p>`
            : nothing
        }
        ${this.renderHealthFacts(health)}
      </section>
    `;
  }

  private renderDeadWakeRow(row: HarnessDeadWake): unknown {
    const key = wakeKey(row);
    const retrying = this.retrying[key] === true;
    const result = this.results[key];
    const canRetry = this.data?.canRetry === true;
    return html`
      <li class="harness-supervisor__wake" data-dead-wake=${row.eventId} data-task-id=${row.taskId}>
        <div class="harness-supervisor__wake-head">
          <div>
            <p class="harness-supervisor__wake-id">原事件 ${row.eventId}</p>
            <p class="harness-supervisor__wake-target">目标 Task ${row.taskId}</p>
          </div>
          ${
            canRetry
              ? html`<button
                  type="button"
                  class="btn harness-supervisor__retry"
                  ?disabled=${retrying}
                  aria-busy=${retrying ? 'true' : 'false'}
                  @click=${() => this.retry(row)}
                >
                  ${retrying ? '重试中…' : '按原事件重试'}
                </button>`
              : nothing
          }
        </div>
        <p class="harness-supervisor__wake-reason">
          失败原因：${row.failureReason ?? '未记录原因'}
        </p>
        <p class="harness-supervisor__wake-time">
          创建时间：${formatHarnessTime(row.createdAt)}
          ${
            row.retriedBy
              ? html` · 上次重试：${row.retriedBy}（${formatHarnessTime(row.retriedAt)}）`
              : nothing
          }
        </p>
        ${
          result?.kind === 'success' && result.message
            ? html`<p
                class="harness-supervisor__result harness-supervisor__result--success"
                role="status"
              >
                ${result.message}
              </p>`
            : nothing
        }
        ${
          result?.kind === 'error'
            ? html`<p
                class="harness-supervisor__result harness-supervisor__result--error"
                role="alert"
              >
                ${result.message}
              </p>`
            : nothing
        }
      </li>
    `;
  }

  private renderDeadWakes() {
    if (!this.data) return nothing;
    const rows = this.data.deadWakes;
    return html`
      <section class="harness-supervisor__section" aria-labelledby="harness-supervisor-wake-title">
        <div class="harness-supervisor__section-header">
          <div>
            <h2 id="harness-supervisor-wake-title">Dead wake 恢复</h2>
            <p class="harness-supervisor__summary">逐条确认，只重试原 event_id；不提供批量重试。</p>
          </div>
        </div>
        ${
          this.data.readOnly
            ? html`<p class="harness-supervisor__readonly">
                只读：需要 Agent 管理权限才能按原事件重试。
              </p>`
            : nothing
        }
        ${
          rows.length === 0
            ? html`<p class="harness-supervisor__empty">没有待恢复的 dead wake。</p>`
            : html`<ul class="harness-supervisor__wake-list">
                ${rows.map((row) => this.renderDeadWakeRow(row))}
              </ul>`
        }
      </section>
    `;
  }

  private renderStatus() {
    if (this.loading) {
      return html`<p class="harness-supervisor__loading" role="status">加载中…</p>`;
    }
    if (!this.error) return nothing;
    return html`<p class="harness-supervisor__error" role="alert">${this.error}</p>`;
  }

  render() {
    return html`
      <section class="harness-supervisor" aria-labelledby="harness-supervisor-title">
        <a class="harness-supervisor__back" href="#/harness">← 返回 Harness 工作台</a>
        <header class="harness-supervisor__header">
          <div>
            <h1 id="harness-supervisor-title" tabindex="-1">Supervisor 健康</h1>
            <p>区分正常空闲与调度故障；下一次唤醒是计划时间，不承诺准点执行。</p>
          </div>
          <button type="button" class="btn" ?disabled=${this.loading} @click=${() => this.load()}>
            刷新
          </button>
        </header>
        <p class="harness-sr-only" role="status" aria-live="polite">${this.announcement}</p>
        ${this.renderStatus()}
        ${this.data ? html`${this.renderHealth()} ${this.renderDeadWakes()}` : nothing}
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-harness-supervisor': HarnessSupervisorView;
  }
}
