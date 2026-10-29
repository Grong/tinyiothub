import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError } from '../../api/client.js';
import { decideRuntimeApproval } from '../../api/runtime.js';
import type { RuntimeApprovalDecisionResult } from '../../types/runtime.js';
import type { RuntimeProjectedApproval } from './runtime-projection.js';
import { safeJson } from './runtime-json.js';

const readOnlyTools = new Set(['read_device', 'list_devices']);

function isRenderable(
  approval: RuntimeProjectedApproval | null | undefined,
): approval is RuntimeProjectedApproval {
  return Boolean(approval) && typeof approval?.id === 'string' && typeof approval.tool === 'string';
}

@customElement('runtime-approval')
export class RuntimeApprovalView extends LitElement {
  @property() workspaceId = '';
  @property() taskId = '';
  @property({ attribute: false }) approval: RuntimeProjectedApproval | null = null;
  @property({ type: Boolean }) readOnly = false;

  @state() private displayApproval: RuntimeProjectedApproval | null = null;
  @state() private pendingDecision: 'approve' | 'reject' | null = null;
  @state() private error = '';

  private generation = 0;
  private controller = new AbortController();

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    if (this.controller.signal.aborted) this.controller = new AbortController();
  }

  disconnectedCallback(): void {
    this.generation += 1;
    this.controller.abort();
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (changed.has('taskId') || changed.has('workspaceId')) this.resetContext();
    if (changed.has('approval')) {
      this.syncApproval(changed.get('approval') as RuntimeProjectedApproval | null | undefined);
    }
  }

  private resetContext(): void {
    this.generation += 1;
    this.controller.abort();
    this.controller = new AbortController();
    this.pendingDecision = null;
    this.error = '';
    this.displayApproval = isRenderable(this.approval) ? this.approval : null;
  }

  private syncApproval(previous: RuntimeProjectedApproval | null | undefined): void {
    const incoming = this.approval;
    if (!incoming) {
      this.displayApproval = null;
      return;
    }
    const local = this.displayApproval;
    // A server decision already acknowledged locally must not be reverted to a
    // stale pending projection delivered by an older snapshot.
    if (
      local &&
      local.id === incoming.id &&
      local.state !== 'pending' &&
      previous?.id === incoming.id &&
      incoming.state === 'pending'
    ) {
      return;
    }
    this.displayApproval = isRenderable(incoming) ? incoming : null;
    // A refreshed non-pending state resolves any earlier 409, so stale failure
    // text must not coexist with the authoritative state.
    if (incoming.state !== undefined && incoming.state !== 'pending') this.error = '';
  }

  get currentApproval(): RuntimeProjectedApproval | null {
    return this.displayApproval;
  }

  private async decide(decision: 'approve' | 'reject'): Promise<void> {
    const approval = this.displayApproval;
    if (this.pendingDecision || !approval || approval.state !== 'pending' || this.readOnly) return;
    const generation = this.generation;
    this.pendingDecision = decision;
    this.error = '';
    try {
      const workspaceId = this.workspaceId || approval.workspaceId || '';
      const taskId = approval.taskId || this.taskId;
      const response: { result: RuntimeApprovalDecisionResult | null } =
        await decideRuntimeApproval(
          workspaceId,
          taskId,
          approval.id,
          decision,
          this.controller.signal,
        );
      if (!response.result) throw new ApiError(500, 'Approval projection missing');
      if (generation !== this.generation) return;
      this.displayApproval = response.result.approval;
      this.dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
    } catch (error) {
      if (generation !== this.generation || this.controller.signal.aborted) return;
      if (error instanceof ApiError && error.code === 409) {
        this.dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
      }
      this.error = error instanceof ApiError ? error.message : 'Approval decision failed';
    } finally {
      if (generation === this.generation) this.pendingDecision = null;
    }
  }

  private renderState(approval: RuntimeProjectedApproval) {
    const state = approval.state ?? 'pending';
    const busy = this.pendingDecision !== null;
    if (state === 'pending') {
      if (this.readOnly) {
        return html`<p class="runtime-approval__state" data-approval-state="pending">
          Pending approval · read-only
        </p>`;
      }
      return html`<div class="runtime-approval__actions">
        <button
          type="button"
          data-action="approve"
          data-executable
          ?disabled=${busy}
          @click=${() => this.decide('approve')}
        >
          ${this.pendingDecision === 'approve' ? 'Approving…' : 'Approve once'}
        </button>
        <button
          type="button"
          data-action="reject"
          data-executable
          ?disabled=${busy}
          @click=${() => this.decide('reject')}
        >
          ${this.pendingDecision === 'reject' ? 'Rejecting…' : 'Reject'}
        </button>
      </div>`;
    }
    if (state === 'approved') {
      return html`<p class="runtime-approval__state" data-approval-state="approved">
        <span>Approved</span
        >${approval.deciderId ? html`<span> · ${approval.deciderId}</span>` : nothing}${approval.decidedAt ? html`<span> · ${approval.decidedAt}</span>` : nothing}
      </p>`;
    }
    if (state === 'rejected') {
      return html`<p class="runtime-approval__state" data-approval-state="rejected">
        <span>Rejected</span
        >${approval.deciderId ? html`<span> · ${approval.deciderId}</span>` : nothing}${approval.decidedAt ? html`<span> · ${approval.decidedAt}</span>` : nothing}
      </p>`;
    }
    if (state === 'expired') {
      return html`<p class="runtime-approval__state" data-approval-state="expired">
        Expired · this approval request is no longer valid.
      </p>`;
    }
    return html`<p class="runtime-approval__state" data-approval-state="unavailable">
      Unavailable · this approval request cannot be decided.
    </p>`;
  }

  render() {
    const approval = this.displayApproval;
    if (!approval || !isRenderable(approval)) return nothing;
    const state = approval.state ?? 'pending';
    const risk = readOnlyTools.has(approval.tool ?? '')
      ? 'Read-only device access'
      : 'Device state change';
    const style = html`<style>
      runtime-approval {
        display: block;
      }
      .runtime-approval {
        margin: var(--space-2, 8px) 0;
        padding: var(--space-3, 12px);
        border: 1px solid var(--warning, #f59e0b);
        border-radius: var(--radius-md, 10px);
        background: var(--warning-subtle, rgba(245, 158, 11, 0.08));
      }
      .runtime-approval__header {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-2, 8px);
        align-items: baseline;
      }
      .runtime-approval__reason {
        margin: var(--space-2, 8px) 0;
      }
      .runtime-approval__meta {
        color: var(--muted-foreground, #94a3b8);
        font-size: 12px;
      }
      .runtime-approval pre {
        max-height: 220px;
        overflow: auto;
        margin: var(--space-2, 8px) 0;
        padding: var(--space-2, 8px);
        border-radius: var(--radius-sm, 6px);
        background: var(--bg-subtle, rgba(255, 255, 255, 0.06));
      }
      .runtime-approval__actions {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-2, 8px);
      }
      .runtime-approval button {
        min-height: 44px;
        min-width: 44px;
        padding: 0 12px;
        border: 1px solid var(--glass-border, rgba(148, 163, 184, 0.3));
        border-radius: var(--radius-md, 10px);
        background: var(--bg-subtle, rgba(255, 255, 255, 0.06));
        color: inherit;
        cursor: pointer;
      }
      .runtime-approval button[data-action='approve'] {
        border-color: var(--accent, #38bdf8);
        font-weight: 600;
      }
      .runtime-approval button:disabled {
        opacity: 0.55;
        cursor: default;
      }
      .runtime-approval__error {
        color: var(--danger, #ef4444);
      }
    </style>`;
    return html`${style}
      <section
        class="runtime-approval"
        data-approval-id=${approval.id}
        data-approval-state=${state}
        aria-label="Approval request"
      >
        <header class="runtime-approval__header">
          <strong>${approval.tool}</strong>
          <span>${approval.deviceId ?? 'Unknown device'}</span>
          <span class="runtime-approval__meta">${risk}</span>
          ${approval.expiresAt ? html`<span class="runtime-approval__meta">Expires ${approval.expiresAt}</span>` : nothing}
        </header>
        ${approval.reason ? html`<p class="runtime-approval__reason">${approval.reason}</p>` : nothing}
        <pre data-approval-args>${safeJson(approval.argsJson)}</pre>
        ${this.renderState(approval)}
        ${this.error ? html`<p role="alert" class="runtime-approval__error">${this.error}</p>` : nothing}
      </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-approval': RuntimeApprovalView;
  }
}
