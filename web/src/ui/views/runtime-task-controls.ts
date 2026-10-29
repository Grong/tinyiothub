import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError } from '../../api/client.js';
import { cancelRuntimeTask, pauseRuntimeTask, resumeRuntimeTask } from '../../api/runtime.js';
import type { TaskProjection, TaskStatus } from '../../types/runtime.js';
import type { RuntimeProjectedJob, RuntimeProjection } from './runtime-projection.js';
import './runtime-confirm-dialog.js';

const pausableStatuses = new Set<TaskStatus>([
  'active', 'waiting_job', 'waiting_watch', 'needs_approval',
]);
const cancellableStatuses = new Set<TaskStatus>([
  'active', 'waiting_job', 'waiting_watch', 'needs_approval', 'paused',
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
    return this.projection?.jobs.filter((job) =>
      job.state === 'accepted' || job.state === 'running') ?? [];
  }

  private get unknownJobs(): RuntimeProjectedJob[] {
    return this.projection?.jobs.filter((job) => job.state === 'cancelled_unknown') ?? [];
  }

  private emitSnapshot(snapshot: TaskProjection): void {
    this.dispatchEvent(new CustomEvent<TaskProjection>('projection-change', {
      detail: snapshot, bubbles: true, composed: true,
    }));
  }

  private restoreOverflowFocus(): void {
    void this.updateComplete.then(() => {
      this.querySelector<HTMLButtonElement>('button[data-action="overflow"]')?.focus();
    });
  }

  private async submitControl(action: 'pause' | 'resume'): Promise<void> {
    if (this.pending || this.readOnly) return;
    if (action === 'pause' ? !this.canPause : !this.canResume) return;
    const generation = this.generation;
    this.pending = action;
    this.error = '';
    try {
      const response = action === 'pause'
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
    try {
      const response = await cancelRuntimeTask(this.workspaceId, this.taskId, this.controller.signal);
      if (!response.result) throw new ApiError(500, 'Task projection missing');
      if (generation !== this.generation) return;
      this.emitSnapshot(response.result);
    } catch (error) {
      if (generation !== this.generation || this.controller.signal.aborted) return;
      this.cancelRequested = false;
      this.error = error instanceof ApiError ? error.message : 'Unable to cancel task';
    } finally {
      if (generation === this.generation) {
        this.pending = null;
        if (this.restoreFocusWhenIdle) {
          this.restoreFocusWhenIdle = false;
          this.restoreOverflowFocus();
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
      <p>${activeJobs} active job${activeJobs === 1 ? '' : 's'} · ${activeWatches} active watch${activeWatches === 1 ? '' : 'es'}</p>
      <p><strong>Affected devices:</strong> ${devices.length ? devices.join(', ') : 'None'}</p>
      <p>Cancelling stops this task's side effects. Unknown device results must be reviewed; they are not reported as stopped.</p>
    `;
  }

  private renderCancelDialog() {
    return html`<runtime-confirm-dialog
      .title=${'Cancel task'}
      .body=${this.renderCancelBody()}
      .primaryLabel=${'Keep running'}
      .dangerLabel=${'Cancel task'}
      @confirm-primary=${this.closeCancel}
      @confirm-danger=${this.confirmCancel}>
    </runtime-confirm-dialog>`;
  }

  private renderAttention(jobs: RuntimeProjectedJob[]) {
    const devices = [...new Set(jobs.map((job) => job.deviceId).filter((id): id is string => Boolean(id)))];
    return html`<section class="runtime-task-controls__attention" role="alert" data-attention="cancelled-unknown">
      <strong>Cancellation needs attention</strong>
      <p>Some device results are unknown${devices.length ? `: ${devices.join(', ')}` : ''}. Review them before retrying.</p>
      <button type="button" data-action="refresh-status" @click=${this.requestRefresh}>Refresh status</button>
    </section>`;
  }

  render() {
    void this.revision;
    const unknown = this.unknownJobs;
    const style = html`<style>
      runtime-task-controls { display: inline-flex; flex-direction: column; gap: var(--space-2, 8px); }
      .runtime-task-controls__actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2, 8px); }
      .runtime-task-controls__note { color: var(--muted-foreground, #94a3b8); font-size: 12px; }
      .runtime-task-controls button { min-height: 44px; min-width: 44px; padding: 0 12px; border: 1px solid var(--glass-border, rgba(148, 163, 184, 0.3)); border-radius: var(--radius-md, 10px); background: var(--bg-subtle, rgba(255, 255, 255, 0.06)); color: inherit; cursor: pointer; }
      .runtime-task-controls button:disabled { opacity: 0.55; cursor: default; }
      .runtime-task-controls__overflow { position: relative; }
      .runtime-task-controls__overflow [role="menu"] { position: absolute; right: 0; z-index: 20; margin-top: 4px; min-width: 160px; padding: 4px; border: 1px solid var(--glass-border, rgba(148, 163, 184, 0.3)); border-radius: var(--radius-md, 10px); background: var(--surface, #111827); }
      .runtime-task-controls__overflow [role="menuitem"] { width: 100%; text-align: left; border-color: var(--danger, #ef4444); color: var(--danger, #ef4444); }
      .runtime-task-controls__attention { padding: var(--space-2, 8px); border: 1px solid var(--warning, #f59e0b); border-radius: var(--radius-md, 10px); color: var(--warning, #f59e0b); }
      .runtime-task-controls__attention p { margin: 4px 0; }
      [data-pending="cancel"] { color: var(--warning, #f59e0b); font-weight: 600; }
    </style>`;

    if (this.readOnly) {
      return html`${style}${unknown.length ? this.renderAttention(unknown) : nothing}`;
    }

    const showCancelling = this.cancelRequested && !this.isTerminal;
    return html`${style}<div class="runtime-task-controls" @keydown=${this.onKeydown}>
      ${unknown.length ? this.renderAttention(unknown) : nothing}
      ${showCancelling
        ? html`<span role="status" data-pending="cancel">${this.pending === 'cancel' ? 'Cancelling…' : 'Cancelling'}</span>`
        : nothing}
      ${this.canPause || this.canResume || this.canCancel
        ? html`<div class="runtime-task-controls__actions">
            ${this.canPause || this.canResume
              ? html`<span class="runtime-task-controls__note">Stops new runs; active jobs and watches continue</span>
                <button type="button" data-action=${this.canResume ? 'resume' : 'pause'} data-executable
                  ?disabled=${this.pending !== null} @click=${() => this.submitControl(this.canResume ? 'resume' : 'pause')}>
                  ${this.pending === 'resume' ? 'Resuming…' : this.pending === 'pause' ? 'Pausing…' : this.canResume ? 'Resume task' : 'Pause task'}
                </button>`
              : nothing}
            ${this.canCancel
              ? html`<div class="runtime-task-controls__overflow">
                  <button type="button" data-action="overflow" aria-haspopup="menu"
                    aria-expanded=${this.menuOpen ? 'true' : 'false'} aria-label="More task actions"
                    ?disabled=${this.pending !== null} @click=${this.toggleMenu}>⋯</button>
                  ${this.menuOpen
                    ? html`<div role="menu">
                        <button type="button" role="menuitem" data-action="menu-cancel" data-executable
                          @click=${this.openCancel}>Cancel task</button>
                      </div>`
                    : nothing}
                </div>`
              : nothing}
          </div>`
        : nothing}
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
