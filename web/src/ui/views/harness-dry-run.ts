/**
 * Isolated Workflow dry-run page (X2/XD3, DR7=A).
 *
 * Entered from one fixed revision. The snapshot picker names both the
 * workspace event sequence and its observed time. The whole page keeps a
 * simulation banner; results reuse the generic Run timeline/evidence/error
 * projection but stay read-only. There is no real-execution or production
 * approval entry, and the only return action goes back to the fixed revision.
 *
 * All requests go through web/src/api/harness.ts. Dynamic text is rendered
 * with Lit interpolation (auto-escaped); never unsafeHTML/innerHTML.
 */

import { LitElement, html, nothing, type PropertyValues, type TemplateResult } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { getWorkspaceId } from '../../api/client.js';
import {
  getHarnessSimulation,
  listHarnessWorkflowSnapshots,
  startHarnessDryRun,
} from '../../api/harness.js';
import type {
  HarnessSimulationProjection,
  HarnessWorkflowSnapshot,
} from '../../types/harness.js';
import { renderRunTimeline } from '../shared/run-timeline.js';
import { formatHarnessTime, renderHarnessStatus } from './harness-status.js';
import { toHarnessDryRunUrl } from './harness-url-state.js';
import './harness-dry-run.css';

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : '请求失败，请重试';
}

@customElement('view-harness-dry-run')
export class HarnessDryRunView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';
  @property() workflowId = '';
  @property({ type: Number }) version = 0;
  @property({ type: Number, attribute: false }) snapshotSeq: number | null = null;
  @property() taskId = '';

  @state() private snapshots: HarnessWorkflowSnapshot[] = [];
  @state() private selectedSeq: number | null = null;
  @state() private loadingSnapshots = false;
  @state() private snapshotError = '';
  @state() private starting = false;
  @state() private startError = '';
  @state() private projection: HarnessSimulationProjection | null = null;
  @state() private projectionLoading = false;
  @state() private projectionError = '';
  @state() private announcement = '';

  private snapshotGeneration = 0;
  private projectionGeneration = 0;
  private snapshotAbort: AbortController | null = null;
  private projectionAbort: AbortController | null = null;

  /** Light DOM so shared Harness tokens and run-timeline styles apply. */
  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.selectedSeq = this.snapshotSeq;
    if (!this.workspaceId || !this.workflowId || this.version <= 0) {
      this.snapshotError = '缺少固定的 Workflow revision，无法开始模拟运行。';
      return;
    }
    void this.loadSnapshots();
    if (this.taskId) void this.loadProjection();
  }

  disconnectedCallback(): void {
    super.disconnectedCallback();
    this.snapshotGeneration += 1;
    this.projectionGeneration += 1;
    this.snapshotAbort?.abort();
    this.snapshotAbort = null;
    this.projectionAbort?.abort();
    this.projectionAbort = null;
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (changed.has('taskId')) {
      const previous = changed.get('taskId');
      if (previous !== undefined && this.taskId) {
        void this.loadProjection();
      }
    }
  }

  private get returnHref(): string {
    // Return to the same fixed revision; the editor loads that version
    // read-only instead of silently following a newer current revision.
    return `#/harness/workflows/${encodeURIComponent(this.workflowId)}?version=${this.version}`;
  }

  private async loadSnapshots(): Promise<void> {
    const generation = ++this.snapshotGeneration;
    this.snapshotAbort?.abort();
    this.snapshotAbort = new AbortController();
    this.loadingSnapshots = true;
    this.snapshotError = '';
    try {
      const response = await listHarnessWorkflowSnapshots(
        this.workspaceId,
        this.workflowId,
        this.version,
        this.snapshotAbort.signal,
      );
      if (generation !== this.snapshotGeneration) return;
      this.snapshots = response.result?.snapshots ?? [];
      if (this.selectedSeq === null && this.snapshots.length === 1) {
        this.selectedSeq = this.snapshots[0].seq;
      }
      if (
        this.selectedSeq !== null &&
        !this.snapshots.some((snapshot) => snapshot.seq === this.selectedSeq)
      ) {
        this.selectedSeq = null;
      }
    } catch (error) {
      if (generation !== this.snapshotGeneration) return;
      this.snapshotError = errorMessage(error);
    } finally {
      if (generation === this.snapshotGeneration) this.loadingSnapshots = false;
    }
  }

  private async loadProjection(): Promise<void> {
    if (!this.taskId) return;
    const generation = ++this.projectionGeneration;
    this.projectionAbort?.abort();
    this.projectionAbort = new AbortController();
    this.projectionLoading = true;
    this.projectionError = '';
    try {
      const response = await getHarnessSimulation(this.workspaceId, this.taskId, this.projectionAbort.signal);
      if (generation !== this.projectionGeneration) return;
      this.projection = response.result;
    } catch (error) {
      if (generation !== this.projectionGeneration) return;
      this.projection = null;
      this.projectionError = errorMessage(error);
    } finally {
      if (generation === this.projectionGeneration) this.projectionLoading = false;
    }
  }

  private selectSnapshot(event: Event): void {
    const value = (event.target as HTMLSelectElement).value;
    this.selectedSeq = value ? Number(value) : null;
  }

  private async start(): Promise<void> {
    if (this.starting || this.selectedSeq === null || !this.workflowId) return;
    this.starting = true;
    this.startError = '';
    try {
      const response = await startHarnessDryRun(
        this.workspaceId,
        this.workflowId,
        this.version,
        this.selectedSeq,
      );
      const result = response.result;
      if (!result) throw new Error('空响应');
      this.taskId = result.taskId;
      window.history.replaceState(
        {},
        '',
        toHarnessDryRunUrl(this.workflowId, this.version, {
          snapshotSeq: this.selectedSeq,
          taskId: result.taskId,
        }),
      );
      this.announcement = `模拟运行已创建：${result.taskId}`;
      // The taskId property change loads the projection once; calling it here
      // as well would race two generations against the same AbortController.
    } catch (error) {
      this.startError = errorMessage(error);
    } finally {
      this.starting = false;
    }
  }

  private renderBanner(): TemplateResult {
    return html`
      <div class="harness-dry-run__banner" data-simulation-banner role="note">
        <span class="harness-dry-run__banner-icon" aria-hidden="true">▶</span>
        <p>
          <strong>模拟运行</strong>
          <span>仅验证配置、提示词、输出与投影；不代表真实模型、设备或物理结果。</span>
        </p>
      </div>
    `;
  }

  private renderPicker(): TemplateResult {
    const empty = !this.loadingSnapshots && !this.snapshotError && this.snapshots.length === 0;
    return html`
      <section class="harness-dry-run__picker" aria-labelledby="harness-dry-run-picker-title">
        <h2 id="harness-dry-run-picker-title">选择设备事实快照</h2>
        <p class="harness-dry-run__hint">
          固定 revision v${this.version}；模拟只读取选中的历史 stamped snapshot，不查询当前设备值。
        </p>
        ${
          this.loadingSnapshots
            ? html`<p role="status">正在加载历史快照…</p>`
            : nothing
        }
        ${
          this.snapshotError
            ? html`<p class="harness-dry-run__error" role="alert">
                加载历史快照失败：${this.snapshotError}
              </p>`
            : nothing
        }
        ${
          empty
            ? html`<p class="harness-dry-run__empty" role="status">
                当前没有可用的设备事实快照；请先完成一次真实运行以生成 stamped snapshot。
              </p>`
            : nothing
        }
        ${
          this.snapshots.length > 0
            ? html`<div class="harness-dry-run__field">
                <label for="dry-run-snapshot">快照序列 / 观测时间</label>
                <select
                  id="dry-run-snapshot"
                  data-field="snapshot"
                  @change=${this.selectSnapshot}
                >
                  ${this.snapshots.map(
                    (snapshot) => html`<option
                      value=${String(snapshot.seq)}
                      ?selected=${snapshot.seq === this.selectedSeq}
                    >
                      ${snapshot.seq} · ${formatHarnessTime(snapshot.observedAt)}
                    </option>`,
                  )}
                </select>
                <p class="harness-dry-run__meta">
                  设备声明 ${this.selectedSnapshot?.declarationHash ?? '暂无'} · 驱动
                  ${this.selectedSnapshot?.driverVersion ?? '暂无'}
                </p>
              </div>`
            : nothing
        }
        ${
          this.startError
            ? html`<p class="harness-dry-run__error" role="alert">
                无法开始模拟运行：${this.startError}
              </p>`
            : nothing
        }
        <div class="harness-dry-run__actions">
          <button
            type="button"
            class="btn btn--primary"
            data-action="start-dry-run"
            ?disabled=${this.starting || this.selectedSeq === null}
            @click=${() => void this.start()}
          >
            ${this.starting ? '正在开始…' : '开始模拟运行'}
          </button>
          <a class="btn" data-action="return-revision" href=${this.returnHref}>返回此版本</a>
        </div>
      </section>
    `;
  }

  private get selectedSnapshot(): HarnessWorkflowSnapshot | null {
    return this.snapshots.find((snapshot) => snapshot.seq === this.selectedSeq) ?? null;
  }

  private renderMeta(projection: HarnessSimulationProjection): TemplateResult {
    return html`
      <dl class="harness-dry-run__summary" data-simulation-meta>
        <div><dt>状态</dt><dd>${renderHarnessStatus(projection.status)}</dd></div>
        <div><dt>执行模式</dt><dd>模拟运行</dd></div>
        <div><dt>Workflow</dt><dd>${projection.workflowId ?? '未知'} · v${projection.workflowVersion ?? '?'}</dd></div>
        <div><dt>模拟 Task</dt><dd>${projection.taskId}</dd></div>
        <div><dt>模拟 Run</dt><dd>${projection.runId ?? '暂无'}</dd></div>
        <div>
          <dt>输入快照</dt>
          <dd>
            ${projection.snapshot.seq ?? '暂无'} ·
            ${formatHarnessTime(projection.snapshot.observedAt)}
          </dd>
        </div>
      </dl>
    `;
  }

  private renderProjection(): TemplateResult {
    if (this.projectionLoading) {
      return html`<p role="status">正在加载模拟结果…</p>`;
    }
    if (this.projectionError) {
      return html`<p class="harness-dry-run__error" role="alert">
        加载模拟结果失败：${this.projectionError}
      </p>`;
    }
    if (!this.projection) {
      return html`<p class="harness-dry-run__empty" role="status">模拟运行结果不存在。</p>`;
    }
    const projection = this.projection;
    const entries = projection.steps.map((step) => ({
      id: step.id,
      name: step.name,
      args: step.args,
      output: step.output,
    }));
    return html`
      <section class="harness-dry-run__result" data-simulation-result aria-labelledby="harness-dry-run-result-title">
        <div class="harness-dry-run__result-header">
          <h2 id="harness-dry-run-result-title">模拟结果</h2>
          <span class="harness-dry-run__readonly" data-readonly>只读</span>
        </div>
        ${projection.objective ? html`<p class="harness-dry-run__objective">${projection.objective}</p>` : nothing}
        ${this.renderMeta(projection)}
        ${
          projection.errors.length > 0
            ? html`<div class="harness-dry-run__errors" role="alert">
                ${projection.errors.map(
                  (error) => html`<p data-simulation-error>
                    <strong>${error.type}</strong> ${error.message}
                  </p>`,
                )}
              </div>`
            : nothing
        }
        <div class="harness-dry-run__steps">
          <h3>步骤</h3>
          ${
            entries.length > 0
              ? renderRunTimeline({ entries, live: false, budgetMax: 0, mode: 'full' })
              : html`<p class="harness-dry-run__empty">本次模拟没有步骤事件。</p>`
          }
        </div>
        ${this.renderOutcome(projection)}
        ${this.renderEvidence(projection)}
        <div class="harness-dry-run__actions">
          <a class="btn" data-action="return-revision" href=${this.returnHref}>返回此版本</a>
        </div>
      </section>
    `;
  }

  private renderOutcome(projection: HarnessSimulationProjection): TemplateResult {
    const outcome = projection.outcome;
    if (!outcome) {
      return html`<p class="harness-dry-run__empty">模拟运行尚未产生结果。</p>`;
    }
    return html`
      <div class="harness-dry-run__outcome">
        <h3>结果</h3>
        <p>${outcome.summary}</p>
        <h4>观察</h4>
        ${
          outcome.observations.length > 0
            ? html`<ul data-observations>
                ${outcome.observations.map((observation) => html`<li>${observation}</li>`)}
              </ul>`
            : html`<p class="harness-dry-run__empty" data-observations>没有观察结果。</p>`
        }
        ${
          outcome.recommendedActions.length > 0
            ? html`<h4>建议动作（模拟，不会执行）</h4>
                <ul data-recommended-actions>
                  ${outcome.recommendedActions.map((action) => html`<li>${action}</li>`)}
                </ul>`
            : nothing
        }
      </div>
    `;
  }

  private renderEvidence(projection: HarnessSimulationProjection): TemplateResult {
    return html`
      <div class="harness-dry-run__evidence">
        <h3>证据</h3>
        ${
          projection.evidence.length > 0
            ? html`<ul>
                ${projection.evidence.map((reference) => html`<li>${reference}</li>`)}
              </ul>`
            : html`<p class="harness-dry-run__empty">本次模拟没有证据引用。</p>`
        }
      </div>
    `;
  }

  render() {
    return html`
      <section class="harness-dry-run" aria-labelledby="harness-dry-run-title">
        <a class="harness-dry-run__back" data-action="return-revision" href=${this.returnHref}>
          ← 返回此版本
        </a>
        ${this.renderBanner()}
        <header class="harness-dry-run__header">
          <div>
            <h1 id="harness-dry-run-title" tabindex="-1">模拟运行</h1>
            <p>
              固定 Workflow revision v${this.version} 的隔离 dry-run。结果只证明配置与投影，
              不证明真实模型、设备驱动或物理结果。
            </p>
          </div>
          ${renderHarnessStatus('simulation')}
        </header>
        ${this.taskId ? this.renderProjection() : this.renderPicker()}
        <p
          class="harness-dry-run__sr-only"
          data-announcer
          role="status"
          aria-live="polite"
          aria-atomic="true"
        >
          ${this.announcement}
        </p>
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-harness-dry-run': HarnessDryRunView;
  }
}
