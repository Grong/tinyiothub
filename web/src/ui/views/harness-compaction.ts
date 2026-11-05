import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import {
  compareHarnessCompaction,
  replaceHarnessCompactionSnapshot,
} from '../../api/harness.js';
import type {
  HarnessCompactionComparison,
  HarnessCompactionStatus,
} from '../../types/harness.js';
import { nextAnnouncement } from './harness-status.js';
import './harness-compaction.css';

interface InlineResult {
  kind: 'success' | 'error';
  message: string;
}

const statusCopy: Record<HarnessCompactionStatus, { label: string; tone: string; detail: string }> = {
  matched: {
    label: '同边界匹配',
    tone: 'success',
    detail: '重建 canonical hash 与同边界快照一致；只有此时才允许替换投影快照。',
  },
  hash_mismatch: {
    label: 'Hash 不匹配',
    tone: 'error',
    detail: '保留最新快照，不会覆盖；请查看差异后重新比较。',
  },
  snapshot_missing: {
    label: '缺少同边界快照',
    tone: 'warning',
    detail: '保留现场，不创建或替换快照；请确认边界后重新比较。',
  },
  snapshot_advanced: {
    label: '快照已更新到更晚边界',
    tone: 'warning',
    detail: '最新快照优先，旧边界比较结果不能覆盖；请重新比较。',
  },
  boundary_mismatch: {
    label: '快照边界不一致',
    tone: 'warning',
    detail: '重建与快照必须使用同一 last_seq；保留最新快照，请重新比较。',
  },
  boundary_unavailable: {
    label: '固定边界尚不可用',
    tone: 'warning',
    detail: '该 seq 还没有被持久事件覆盖；不会替换快照，请重新选择边界。',
  },
  stale_append: {
    label: '比较期间出现新事件',
    tone: 'warning',
    detail: '重建期间有新 append；保留最新快照，不自动重试，请重新比较。',
  },
};

function formatValue(value: unknown): string {
  if (value === undefined) return '（缺失）';
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

@customElement('view-harness-compaction')
export class HarnessCompactionView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';

  @state() private projectionName = 'supervisor';
  @state() private subjectId = '';
  @state() private lastSeq = '0';
  @state() private projectionVersion = '1';
  @state() private comparison: HarnessCompactionComparison | null = null;
  @state() private loading = false;
  @state() private error = '';
  @state() private replacing = false;
  @state() private result: InlineResult | null = null;
  @state() private replacedStateVersion: number | null = null;
  @state() private announcement = '';

  private generation = 0;
  private abort: AbortController | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    if (!this.subjectId) {
      this.subjectId = `supervisor:${this.workspaceId}`;
    }
  }

  disconnectedCallback(): void {
    this.generation += 1;
    this.abort?.abort();
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (!changed.has('workspaceId')) return;
    const previous = changed.get('workspaceId') ?? '';
    const defaultSubject = `supervisor:${previous}`;
    if (!this.subjectId || this.subjectId === defaultSubject) {
      this.subjectId = `supervisor:${this.workspaceId}`;
    }
    this.generation += 1;
    this.abort?.abort();
    this.comparison = null;
    this.result = null;
    this.error = '';
    this.replacedStateVersion = null;
    this.replacing = false;
  }

  /**
   * A validation failure invalidates any previous matched comparison so its
   * 「替换投影快照」action cannot remain available for a different boundary.
   */
  private invalidate(message: string): void {
    this.comparison = null;
    this.result = null;
    this.replacedStateVersion = null;
    this.error = message;
  }

  private onCompare = (event: Event): void => {
    event.preventDefault();
    void this.compare();
  };

  private async compare(): Promise<void> {
    const projectionName = this.projectionName.trim();
    const subjectId = this.subjectId.trim();
    const lastSeq = Number(this.lastSeq);
    const projectionVersion = Number(this.projectionVersion);
    if (!this.lastSeq.trim() || !Number.isSafeInteger(lastSeq) || lastSeq < 0) {
      this.invalidate('请输入固定的 last_seq（非负整数）');
      return;
    }
    if (!Number.isSafeInteger(projectionVersion) || projectionVersion <= 0) {
      this.invalidate('projection_version 必须是正整数');
      return;
    }
    if (!projectionName) {
      this.invalidate('请输入 projection_name');
      return;
    }
    if (!subjectId) {
      this.invalidate('请输入 subject_id');
      return;
    }
    if (!this.workspaceId) {
      this.invalidate('未选择工作空间');
      return;
    }

    const generation = ++this.generation;
    this.abort?.abort();
    this.abort = new AbortController();
    this.loading = true;
    this.error = '';
    this.result = null;
    this.replacedStateVersion = null;
    this.comparison = null;
    try {
      const response = await compareHarnessCompaction(
        this.workspaceId,
        { projectionName, subjectId, lastSeq, projectionVersion },
        this.abort.signal,
      );
      if (generation !== this.generation) return;
      const comparison = response.result;
      if (!comparison) throw new Error('空响应');
      this.comparison = comparison;
      this.announcement = nextAnnouncement(
        this.announcement,
        `比较完成：${statusCopy[comparison.status].label}`,
      );
    } catch (error) {
      if (generation !== this.generation || this.abort.signal.aborted) return;
      this.error = error instanceof ApiError ? error.message : '比较失败';
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }

  private onReplace = (): void => {
    void this.replace();
  };

  private async replace(): Promise<void> {
    const comparison = this.comparison;
    if (
      !comparison ||
      !comparison.matched ||
      !comparison.canReplace ||
      this.replacing ||
      this.replacedStateVersion !== null ||
      comparison.snapshotStateVersion === null
    ) {
      return;
    }
    const generation = this.generation;
    this.replacing = true;
    this.result = null;
    try {
      const response = await replaceHarnessCompactionSnapshot(this.workspaceId, {
        projectionName: comparison.projectionName,
        subjectId: comparison.subjectId,
        lastSeq: comparison.boundaryLastSeq,
        projectionVersion: comparison.projectionVersion,
        expectedLastSeq: comparison.boundaryLastSeq,
        expectedStateVersion: comparison.snapshotStateVersion,
      });
      // A workspace switch invalidates this response; it must not repopulate
      // the new workspace with the old comparison's replacement result.
      if (generation !== this.generation) return;
      const snapshot = response.result;
      if (!snapshot) throw new Error('空响应');
      this.replacedStateVersion = snapshot.stateVersion;
      this.comparison = { ...comparison, snapshotStateVersion: snapshot.stateVersion };
      this.result = {
        kind: 'success',
        message: `已替换投影快照：边界 seq ${snapshot.lastSeq}，state_version ${snapshot.stateVersion}。`,
      };
      this.announcement = nextAnnouncement(
        this.announcement,
        `已替换投影快照，边界 seq ${snapshot.lastSeq}`,
      );
    } catch (error) {
      if (generation !== this.generation) return;
      const message = error instanceof ApiError ? error.message : '替换投影快照失败';
      // A 409 carries the fresh server-side comparison so a mismatch can show
      // its locatable differences without auto-retrying.
      const fresh = (error instanceof ApiError ? error.data?.result : null) as
        | HarnessCompactionComparison
        | null
        | undefined;
      if (fresh?.status && fresh.differences) {
        this.comparison = { ...fresh, canReplace: false, readOnly: comparison.readOnly };
      }
      this.result = {
        kind: 'error',
        message: `${message}；保留最新快照，请重新比较。`,
      };
      this.announcement = nextAnnouncement(this.announcement, '替换失败，保留最新快照');
    } finally {
      this.replacing = false;
    }
  }

  private renderFields() {
    return html`
      <form class="harness-compaction__form" data-compaction-form @submit=${this.onCompare}>
        <label class="harness-compaction__field">
          <span>projection_name</span>
          <input
            type="text"
            data-field="projection-name"
            .value=${this.projectionName}
            @input=${(event: Event) => {
              this.projectionName = (event.target as HTMLInputElement).value;
            }}
          />
        </label>
        <label class="harness-compaction__field">
          <span>subject_id</span>
          <input
            type="text"
            data-field="subject-id"
            .value=${this.subjectId}
            @input=${(event: Event) => {
              this.subjectId = (event.target as HTMLInputElement).value;
            }}
          />
        </label>
        <label class="harness-compaction__field">
          <span>固定边界 last_seq</span>
          <input
            type="number"
            min="0"
            step="1"
            data-field="last-seq"
            .value=${this.lastSeq}
            @input=${(event: Event) => {
              this.lastSeq = (event.target as HTMLInputElement).value;
            }}
          />
        </label>
        <label class="harness-compaction__field">
          <span>projection_version</span>
          <input
            type="number"
            min="1"
            step="1"
            data-field="projection-version"
            .value=${this.projectionVersion}
            @input=${(event: Event) => {
              this.projectionVersion = (event.target as HTMLInputElement).value;
            }}
          />
        </label>
        <button
          type="submit"
          class="btn btn--primary harness-compaction__compare"
          data-action="compare"
          ?disabled=${this.loading}
          aria-busy=${this.loading ? 'true' : 'false'}
        >
          ${this.loading ? '比较中…' : '比较'}
        </button>
      </form>
    `;
  }

  private renderStatus(comparison: HarnessCompactionComparison) {
    const copy = statusCopy[comparison.status];
    return html`
      <div class="harness-compaction__status harness-compaction__status--${copy.tone}" data-compaction-status>
        <strong>${copy.label}</strong>
        <span>${copy.detail}</span>
      </div>
    `;
  }

  private renderFacts(comparison: HarnessCompactionComparison) {
    const range =
      comparison.seqFirst === null || comparison.seqLast === null
        ? '无事件'
        : `seq ${comparison.seqFirst} – ${comparison.seqLast}`;
    return html`
      <dl class="harness-compaction__facts" data-compaction-facts>
        <div>
          <dt>固定边界</dt>
          <dd>last_seq ${comparison.boundaryLastSeq}</dd>
        </div>
        <div>
          <dt>seq 范围</dt>
          <dd>${range}</dd>
        </div>
        <div>
          <dt>projection_version</dt>
          <dd>${comparison.projectionVersion}</dd>
        </div>
        <div>
          <dt>last complete seq</dt>
          <dd>${comparison.lastCompleteSeq}</dd>
        </div>
        <div>
          <dt>观察高水位</dt>
          <dd>${comparison.observedHighWatermark}</dd>
        </div>
        <div>
          <dt>快照边界</dt>
          <dd>${comparison.snapshotLastSeq === null ? '无快照' : `last_seq ${comparison.snapshotLastSeq}`}</dd>
        </div>
        <div>
          <dt>快照 state_version</dt>
          <dd>${comparison.snapshotStateVersion ?? '无快照'}</dd>
        </div>
        <div>
          <dt>比较期间新 append</dt>
          <dd>${comparison.appendedDuringCompare ? '是' : '否'}</dd>
        </div>
      </dl>
    `;
  }

  private renderHashes(comparison: HarnessCompactionComparison) {
    return html`
      <div class="harness-compaction__hashes">
        <p>
          <span>canonical hash（临时重建）</span>
          <code data-canonical-hash>${comparison.canonicalHash}</code>
        </p>
        <p>
          <span>canonical hash（同边界快照）</span>
          <code data-snapshot-hash>${comparison.snapshotCanonicalHash ?? '无快照'}</code>
        </p>
      </div>
    `;
  }

  private renderEvidence(comparison: HarnessCompactionComparison) {
    return html`
      <section class="harness-compaction__section" aria-labelledby="harness-compaction-evidence-title">
        <h2 id="harness-compaction-evidence-title">证据引用</h2>
        ${
          comparison.evidenceRefs.length === 0
            ? html`<p class="harness-compaction__empty">该边界没有生产事件。</p>`
            : html`<ul class="harness-compaction__evidence">
                ${comparison.evidenceRefs.map(
                  (evidence) => html`<li data-evidence-seq=${evidence.seq}>
                    <code>${evidence.reference}</code>
                    <span>${evidence.eventType}</span>
                  </li>`,
                )}
              </ul>`
        }
      </section>
    `;
  }

  private renderDifferences(comparison: HarnessCompactionComparison) {
    if (comparison.differences.length === 0) return nothing;
    return html`
      <section class="harness-compaction__section" aria-labelledby="harness-compaction-diff-title">
        <h2 id="harness-compaction-diff-title">可定位差异</h2>
        <ul class="harness-compaction__differences" data-differences>
          ${comparison.differences.map(
            (difference) => html`<li>
              <code>${difference.path}</code>
              <span>重建：${formatValue(difference.rebuilt)}</span>
              <span>快照：${formatValue(difference.snapshot)}</span>
            </li>`,
          )}
        </ul>
      </section>
    `;
  }

  private renderActions(comparison: HarnessCompactionComparison) {
    const replaced = this.replacedStateVersion !== null;
    return html`
      <div class="harness-compaction__actions">
        ${
          comparison.matched && comparison.canReplace
            ? html`<button
                type="button"
                class="btn btn--primary"
                data-action="replace"
                ?disabled=${this.replacing || replaced}
                aria-busy=${this.replacing ? 'true' : 'false'}
                @click=${this.onReplace}
              >
                ${replaced ? '已替换' : this.replacing ? '替换中…' : '替换投影快照'}
              </button>`
            : nothing
        }
        <button
          type="button"
          class="btn"
          data-action="recompare"
          ?disabled=${this.loading}
          @click=${() => this.compare()}
        >
          重新比较
        </button>
      </div>
    `;
  }

  private renderComparison(comparison: HarnessCompactionComparison) {
    return html`
      <section class="harness-compaction__section" aria-labelledby="harness-compaction-result-title">
        <h2 id="harness-compaction-result-title">固定边界比较</h2>
        ${this.renderStatus(comparison)} ${this.renderFacts(comparison)} ${this.renderHashes(comparison)}
      </section>
      ${this.renderEvidence(comparison)} ${this.renderDifferences(comparison)}
      ${
        !comparison.matched
          ? html`<p class="harness-compaction__latest">
              保留最新快照，不会以旧比较覆盖；需要时请点击「重新比较」。
            </p>`
          : nothing
      }
      ${
        comparison.readOnly
          ? html`<p class="harness-compaction__readonly" data-compaction-readonly>
              只读：需要 Agent 管理权限才能替换投影快照。
            </p>`
          : nothing
      }
      ${
        this.result
          ? html`<p
              class="harness-compaction__result harness-compaction__result--${this.result.kind}"
              data-compaction-result
              role=${this.result.kind === 'error' ? 'alert' : 'status'}
            >
              ${this.result.message}
            </p>`
          : nothing
      }
      ${this.renderActions(comparison)}
    `;
  }

  private renderStatusMessage() {
    if (this.loading) {
      return html`<p class="harness-compaction__loading" role="status">比较中…</p>`;
    }
    if (!this.error) return nothing;
    return html`<p class="harness-compaction__error" data-compaction-error role="alert">${this.error}</p>`;
  }

  render() {
    return html`
      <section class="harness-compaction" aria-labelledby="harness-compaction-title">
        <a class="harness-compaction__back" href="#/harness">← 返回 Harness 工作台</a>
        <header class="harness-compaction__header">
          <div>
            <h1 id="harness-compaction-title" tabindex="-1">压缩检查器</h1>
            <p>
              在固定 projection_version + last_seq 边界重建临时投影，比较 canonical hash；
              只有同边界匹配才允许 CAS 替换。事实、Task/Run、Effect/Decision 与外部系统全程只读。
            </p>
          </div>
        </header>
        <p class="harness-sr-only" role="status" aria-live="polite">${this.announcement}</p>
        ${this.renderFields()} ${this.renderStatusMessage()}
        ${this.comparison ? this.renderComparison(this.comparison) : nothing}
      </section>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-harness-compaction': HarnessCompactionView;
  }
}
