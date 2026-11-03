import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError } from '../../api/client.js';
import { cancelRuntimeTask, pauseRuntimeTask, resumeRuntimeTask } from '../../api/runtime.js';
import type { TaskProjection, TaskStatus } from '../../types/runtime.js';
import type { RuntimeProjectedJob, RuntimeProjection } from './runtime-projection.js';
import './runtime-confirm-dialog.js';

const pausableStatuses = new Set<TaskStatus>([
  'active',
  'waiting_job',
  'waiting_watch',
  'needs_approval',
]);
const cancellableStatuses = new Set<TaskStatus>([
  'active',
  'waiting_job',
  'waiting_watch',
  'needs_approval',
  'paused',
]);
const terminalStatuses = new Set<TaskStatus>(['completed', 'failed', 'cancelled']);

function watchDeviceId(condition: unknown): string | null {
  if (!condition || typeof condition !== 'object') return null;
  const record = condition as Record<string, unknown>;
  for (const key of ['thingId', 'thing_id', 'deviceId', 'device_id']) {
    const value = record[key];
    if (typeof value === 'string' && value) return value;
  }
  return null;
}

@customElement('runtime-task-controls')
export class RuntimeTaskControls extends LitElement {
  @property() workspaceId = '';
  @property() taskId = '';
  @property({ attribute: false }) projection: RuntimeProjection | null = null;
  @property({ type: Number }) revision = 0;
  @property({ type: Boolean }) readOnly = false;

  @state() private pending: 'pause' | 'resume' | 'cancel' | null = null;
  @state() private error = '';
  @state() private menuOpen = false;
  @state() private cancelOpen = false;
  @state() private cancelRequested = false;

  private generation = 0;
  private controller = new AbortController();
  private restoreFocusWhenIdle = false;

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
    if (changed.has('taskId') || changed.has('workspaceId')) this.resetRequests();
  }

  private resetRequests(): void {
    this.generation += 1;
    this.controller.abort();
    this.controller = new AbortController();
    this.pending = null;
    this.error = '';
    this.menuOpen = false;
    this.cancelOpen = false;
    this.cancelRequested = false;
    this.restoreFocusWhenIdle = false;
  }

  private get status(): TaskStatus | null {
    return this.projection?.task.status ?? null;
  }

  private get isTerminal(): boolean {
    return this.status !== null && terminalStatuses.has(this.status);
  }

  private get canPause(): boolean {
    return this.status !== null && pausableStatuses.has(this.status);
  }

  private get canResume(): boolean {
    return this.status === 'paused';
  }

  private get canCancel(): boolean {
    return this.status !== null && cancellableStatuses.has(this.status) && !this.isTerminal;
  }

  private get activeJobs(): RuntimeProjectedJob[] {
    return (
      this.projection?.jobs.filter((job) => job.state === 'accepted' || job.state === 'running') ??
      []
    );
  }

  private get unknownJobs(): RuntimeProjectedJob[] {
    return this.projection?.jobs.filter((job) => job.state === 'cancelled_unknown') ?? [];
  }

  private emitSnapshot(snapshot: TaskProjection): void {
    this.dispatchEvent(
      new CustomEvent<TaskProjection>('projection-change', {
        detail: snapshot,
        bubbles: true,
        composed: true,
      }),
    );
  }

  private restoreOverflowFocus(): void {
    void this.updateComplete.then(() => {
      const trigger = this.querySelector<HTMLButtonElement>('button[data-action="overflow"]');
      if (trigger) {
        trigger.focus();
        return;
      }
      // A successful cancel can remove the overflow trigger; ask the owning
      // Task view for a stable surviving focus target instead of dropping to body.
      this.dispatchEvent(
        new CustomEvent('focus-status-request', { bubbles: true, composed: true }),
      );
    });
  }

  private async submitControl(action: 'pause' | 'resume'): Promise<void> {
    if (this.pending || this.readOnly) return;
    if (action === 'pause' ? !this.canPause : !this.canResume) return;
    const generation = this.generation;
    this.pending = action;
    this.error = '';
    try {
      const response =
        action === 'pause'
          ? await pauseRuntimeTask(this.workspaceId, this.taskId, this.controller.signal)
          : await resumeRuntimeTask(this.workspaceId, this.taskId, this.controller.signal);
      if (!response.result) throw new ApiError(500, 'Task projection missing');
      if (generation !== this.generation) return;
      this.emitSnapshot(response.result);
    } catch (error) {
      if (generation !== this.generation || this.controller.signal.aborted) return;
      this.error = error instanceof ApiError ? error.message : `Unable to ${action} task`;
    } finally {
      if (generation === this.generation) this.pending = null;
    }
  }

  private toggleMenu(): void {
    if (this.readOnly || this.pending || !this.canCancel) return;
    this.menuOpen = !this.menuOpen;
  }

  private closeMenu(): void {
    this.menuOpen = false;
    this.restoreOverflowFocus();
  }

  private openCancel(): void {
    if (this.readOnly || this.pending || !this.canCancel) return;
    this.menuOpen = false;
    this.cancelOpen = true;
  }

  private closeCancel(): void {
    this.cancelOpen = false;
    this.restoreOverflowFocus();
  }

  private async confirmCancel(): Promise<void> {
    if (this.pending || this.readOnly || !this.canCancel) return;
    const generation = this.generation;
    this.cancelOpen = false;
    this.pending = 'cancel';
    this.cancelRequested = true;
    this.restoreFocusWhenIdle = true;
    this.error = '';
    let restored = false;
    try {
      const response = await cancelRuntimeTask(
        this.workspaceId,
        this.taskId,
        this.controller.signal,
      );
      if (!response.result) throw new ApiError(500, 'Task projection missing');
      if (generation !== this.generation) return;
      this.emitSnapshot(response.result);
      restored = true;
      this.dispatchEvent(
        new CustomEvent('focus-status-request', { bubbles: true, composed: true }),
      );
    } catch (error) {
      if (generation !== this.generation || this.controller.signal.aborted) return;
      this.cancelRequested = false;
      this.error = error instanceof ApiError ? error.message : 'Unable to cancel task';
    } finally {
      if (generation === this.generation) {
        this.pending = null;
        if (this.restoreFocusWhenIdle) {
          this.restoreFocusWhenIdle = false;
          if (!restored) this.restoreOverflowFocus();
        }
      }
    }
  }

  private onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && this.menuOpen) {
      event.preventDefault();
      event.stopPropagation();
      this.closeMenu();
    }
  }

  private requestRefresh(): void {
    this.dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
  }

  private affectedDeviceNames(): string[] {
    const names = new Set<string>();
    for (const job of this.activeJobs) if (job.deviceId) names.add(job.deviceId);
    for (const watch of this.projection?.watches ?? []) {
      const deviceId = watchDeviceId(watch.condition);
      if (deviceId) names.add(deviceId);
    }
    return [...names];
  }

  private renderCancelBody() {
    const objective = this.projection?.task.objective ?? '';
    const activeJobs = this.activeJobs.length;
    const activeWatches = this.projection?.watches.length ?? 0;
    const devices = this.affectedDeviceNames();
    return html`
      <p><strong>Objective:</strong> ${objective}</p>
      <p>
        ${activeJobs} active job${activeJobs === 1 ? '' : 's'} · ${activeWatches} active
        watch${activeWatches === 1 ? '' : 'es'}
      </p>
      <p><strong>Affected devices:</strong> ${devices.length ? devices.join(', ') : 'None'}</p>
      <p>
        Cancelling stops this task's side effects. Unknown device results must be reviewed; they are
        not reported as stopped.
      </p>
    `;
  }

  private renderCancelDialog() {
    return html`<runtime-confirm-dialog
      .title=${'Cancel task'}
      .body=${this.renderCancelBody()}
      .primaryLabel=${'Keep running'}
      .dangerLabel=${'Cancel task'}
      @confirm-primary=${this.closeCancel}
      @confirm-danger=${this.confirmCancel}
    >
    </runtime-confirm-dialog>`;
  }

  private renderAttention(jobs: RuntimeProjectedJob[]) {
    const devices = [
      ...new Set(jobs.map((job) => job.deviceId).filter((id): id is string => Boolean(id))),
    ];
    return html`<section
      class="runtime-task-controls__attention"
      role="alert"
      data-attention="cancelled-unknown"
    >
      <strong>Cancellation needs attention</strong>
      <p>
        Some device results are unknown${devices.length ? `: ${devices.join(', ')}` : ''}. Review
        them before retrying.
      </p>
      ${
        this.readOnly
          ? nothing
          : html`<button class="unstyled" type="button" data-action="refresh-status" @click=${this.requestRefresh}>
              Refresh status
            </button>`
      }
    </section>`;
  }

  // Prettier's Lit HTML printer oscillates on these nested conditionals.
  // prettier-ignore
  render() {
    void this.revision;
    const unknown = this.unknownJobs;
    const style = html`<style>
      runtime-task-controls {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 4px;
        max-width: 300px;
      }
      .runtime-task-controls {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 4px;
        max-width: 100%;
      }
      .runtime-task-controls__actions {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        justify-content: flex-end;
        gap: var(--space-2, 8px);
      }
      .runtime-task-controls__note {
        flex-basis: 100%;
        margin: 0;
        color: var(--muted-foreground, #94a3b8);
        font-size: 11px;
        line-height: 1.4;
        text-align: right;
      }
      .runtime-task-controls button {
        min-height: 36px;
        min-width: 44px;
        padding: 0 14px;
        border: 1px solid var(--border, rgba(148, 163, 184, 0.3));
        border-radius: var(--radius-md, 10px);
        background: transparent;
        color: var(--text, inherit);
        font: inherit;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
      }
      .runtime-task-controls button:hover:not(:disabled) {
        border-color: var(--border-strong, rgba(148, 163, 184, 0.55));
        background: var(--bg-elevated, rgba(255, 255, 255, 0.04));
      }
      .runtime-task-controls button:focus-visible,
      .runtime-task-controls [role='menuitem']:focus-visible {
        outline: 2px solid var(--accent, #0098ff);
        outline-offset: 2px;
      }
      .runtime-task-controls button:disabled {
        opacity: 0.55;
        cursor: default;
      }
      .runtime-task-controls__overflow {
        position: relative;
      }
      .runtime-task-controls__overflow > button:not([role='menuitem']) {
        min-width: 36px;
        padding: 0 10px;
        border-color: transparent;
        color: var(--muted-foreground, #94a3b8);
        font-size: 18px;
        line-height: 1;
      }
      .runtime-task-controls__overflow > button[aria-expanded='true'] {
        border-color: var(--border, rgba(148, 163, 184, 0.3));
        background: var(--bg-elevated, rgba(255, 255, 255, 0.04));
      }
      .runtime-task-controls__overflow [role='menu'] {
        position: absolute;
        right: 0;
        z-index: 20;
        margin-top: 4px;
        min-width: 160px;
        padding: 4px;
        border: 1px solid var(--border-strong, rgba(148, 163, 184, 0.55));
        border-radius: var(--radius-md, 10px);
        background: var(--card, #111827);
        box-shadow: var(--shadow-lg, 0 12px 32px rgba(0, 0, 0, 0.35));
      }
      .runtime-task-controls__overflow [role='menuitem'] {
        width: 100%;
        min-height: 40px;
        padding: 0 10px;
        border: 0;
        border-radius: var(--radius-sm, 6px);
        background: transparent;
        color: var(--danger, #ef4444);
        text-align: left;
      }
      .runtime-task-controls__overflow [role='menuitem']:hover {
        background: var(--danger-subtle, rgba(239, 68, 68, 0.12));
      }
      .runtime-task-controls__attention {
        width: 100%;
        padding: var(--space-2, 8px) var(--space-3, 12px);
        border: 1px solid var(--warning, #f59e0b);
        border-radius: var(--radius-md, 10px);
        background: var(--warn-subtle, rgba(245, 158, 11, 0.12));
        color: var(--text, inherit);
        font-size: 12px;
        text-align: left;
      }
      .runtime-task-controls__attention strong {
        color: var(--warning, #f59e0b);
      }
      .runtime-task-controls__attention p {
        margin: 4px 0 8px;
        line-height: 1.5;
      }
      .runtime-task-controls__attention button {
        min-height: 32px;
        background: transparent;
      }
      .runtime-task-controls__error {
        margin: 0;
        color: var(--danger, #ef4444);
        font-size: 12px;
        text-align: right;
      }
      [data-pending='cancel'] {
        color: var(--warning, #f59e0b);
        font-size: 12px;
        font-weight: 600;
      }
      @media (max-width: 979px) {
        runtime-task-controls {
          align-items: flex-start;
          max-width: 100%;
        }
        .runtime-task-controls {
          align-items: flex-start;
        }
        .runtime-task-controls__actions {
          justify-content: flex-start;
        }
        .runtime-task-controls__note {
          text-align: left;
        }
        .runtime-task-controls__error {
          text-align: left;
        }
      }
    </style>`;

    if (this.readOnly) {
      return html`${style}${unknown.length ? this.renderAttention(unknown) : nothing}`;
    }

    const showCancelling = this.cancelRequested && !this.isTerminal;
    return html`${style}
      <div class="runtime-task-controls" @keydown=${this.onKeydown}>
        ${unknown.length ? this.renderAttention(unknown) : nothing}
        ${
          showCancelling
            ? html`<span role="status" data-pending="cancel"
                >${this.pending === 'cancel' ? 'Cancelling…' : 'Cancelling'}</span
              >`
            : nothing
        }
        ${
          this.canPause || this.canResume || this.canCancel
            ? html`<div class="runtime-task-controls__actions">
                ${
                this.canPause || this.canResume
                  ? html`<span class="runtime-task-controls__note"
                        >Stops new runs; active jobs and watches continue</span
                      >
                      <button
                        class="unstyled"
                        type="button"
                        data-action=${this.canResume ? 'resume' : 'pause'}
                        data-executable
                        ?disabled=${this.pending !== null}
                        @click=${() => this.submitControl(this.canResume ? 'resume' : 'pause')}
                      >
                        ${this.pending === 'resume' ? 'Resuming…' : this.pending === 'pause' ? 'Pausing…' : this.canResume ? 'Resume task' : 'Pause task'}
                      </button>`
                  : nothing
              }
                ${
                this.canCancel
                  ? html`<div class="runtime-task-controls__overflow">
                      <button
                        class="unstyled"
                        type="button"
                        data-action="overflow"
                        aria-haspopup="menu"
                        aria-expanded=${this.menuOpen ? 'true' : 'false'}
                        aria-label="More task actions"
                        ?disabled=${this.pending !== null}
                        @click=${this.toggleMenu}
                      >
                        ⋯
                      </button>
                      ${
                      this.menuOpen
                        ? html`<div role="menu">
                            <button
                              class="unstyled"
                              type="button"
                              role="menuitem"
                              data-action="menu-cancel"
                              data-executable
                              @click=${this.openCancel}
                            >
                              Cancel task
                            </button>
                          </div>`
                        : nothing
                    }
                    </div>`
                  : nothing
              }
              </div>`
            : nothing
        }
        ${this.error ? html`<p role="alert" class="runtime-task-controls__error">${this.error}</p>` : nothing}
        ${this.cancelOpen ? this.renderCancelDialog() : nothing}
      </div>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-task-controls': RuntimeTaskControls;
  }
}
