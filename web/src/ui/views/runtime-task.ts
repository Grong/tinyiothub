import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { getRuntimeTask } from '../../api/runtime.js';
import { connectRuntimeStream } from '../../api/runtime-stream.js';
import type { PlaybackDescriptor, TaskProjection } from '../../types/runtime.js';
import { RuntimeProjection, type RuntimeConnection } from './runtime-projection.js';
import type { RuntimeHistory, RuntimeView } from './runtime-url-state.js';
import './runtime-playback.js';
import type { RuntimePlayback } from './runtime-playback.js';

@customElement('view-runtime-task')
export class RuntimeTaskView extends LitElement {
  @property() workspaceId = getWorkspaceId() ?? '';
  @property() taskId = '';
  @property() view: RuntimeView = 'loop';
  @property({ type: String }) device: string | null = null;
  @property({ attribute: false }) history: RuntimeHistory | null = null;
  @property({ attribute: false }) descriptor?: PlaybackDescriptor;

  @state() private projection: RuntimeProjection | null = null;
  @state() private connection: RuntimeConnection = 'connecting';
  @state() private error = '';
  @state() private revision = 0;

  private generation = 0;
  private abort: AbortController | null = null;
  private closeStream: (() => void) | null = null;

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    if (this.taskId) void this.load();
  }

  disconnectedCallback(): void {
    this.cleanupTask();
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (changed.has('taskId') && changed.get('taskId') !== undefined) {
      this.cleanupTask();
      if (this.taskId) void this.load();
    }
  }

  private cleanupTask(): void {
    this.generation += 1;
    this.closeStream?.();
    this.closeStream = null;
    this.abort?.abort();
    this.abort = null;
    this.querySelector<RuntimePlayback>('runtime-playback')?.destroy();
    this.projection = null;
  }

  private async load(): Promise<void> {
    const generation = ++this.generation;
    const taskId = this.taskId;
    this.abort = new AbortController();
    this.connection = 'connecting';
    this.error = '';
    try {
      const response = await getRuntimeTask(this.workspaceId, taskId, this.abort.signal);
      if (generation !== this.generation || taskId !== this.taskId) return;
      if (!response.result) throw new ApiError(500, 'Task response missing');
      this.installSnapshot(response.result, response.result.cursor);
      if (!this.history) this.openStream(generation, taskId);
    } catch (error) {
      if (generation !== this.generation || this.abort?.signal.aborted) return;
      this.connection = 'offline';
      this.error = error instanceof ApiError ? error.message : 'Network error loading Runtime task';
    }
  }

  private installSnapshot(snapshot: TaskProjection, cursor: number): void {
    if (this.projection) this.projection.replaceWithSnapshot(snapshot, cursor);
    else this.projection = new RuntimeProjection(snapshot, { history: this.history !== null });
    this.revision += 1;
  }

  private openStream(generation: number, taskId: string): void {
    this.closeStream = connectRuntimeStream({
      workspaceId: this.workspaceId,
      taskId,
      after: this.projection?.cursor ?? 0,
      isCurrent: () => generation === this.generation && taskId === this.taskId,
      callbacks: {
        snapshot: (snapshot, cursor) => {
          if (generation !== this.generation) return;
          this.installSnapshot(snapshot, cursor);
        },
        event: (event) => {
          if (generation !== this.generation) return;
          this.projection?.apply(event);
          this.revision += 1;
        },
        resync: (snapshot, cursor) => {
          if (generation !== this.generation) return;
          this.installSnapshot(snapshot, cursor);
        },
        connection: (connection) => {
          if (generation !== this.generation) return;
          this.connection = connection;
          this.projection?.setConnection(connection);
          this.revision += 1;
        },
      },
    });
  }

  private renderProjection() {
    const projection = this.projection;
    if (!projection) return nothing;
    // Mutations are owned by RuntimeProjection; revision only notifies Lit.
    void this.revision;
    return html`
      ${this.descriptor
        ? html`<runtime-playback .descriptor=${this.descriptor}></runtime-playback>`
        : nothing}
      <section aria-label="Task projection" style="margin-top: var(--space-4);">
        <div data-projection="steps">Steps: ${projection.steps.length}</div>
        <div data-projection="devices">Devices: ${projection.devices.map((device) => device.deviceId).join(', ') || 'None'}</div>
        <div data-projection="jobs">Jobs: ${projection.jobs.length}</div>
        <div data-projection="approvals">Approvals: ${projection.approvals.length}</div>
        <div data-projection="watches">Watches: ${projection.watches.length}</div>
        <div data-projection="evidence">Evidence: ${projection.evidence.length}</div>
      </section>
    `;
  }

  render() {
    const projection = this.projection;
    const pendingApprovals = projection?.approvals.filter((approval) => approval.state === 'pending').length ?? 0;
    return html`
      <article style="max-width: 960px;" aria-busy=${projection ? 'false' : 'true'}>
        <nav aria-label="Runtime task navigation" style="display: flex; gap: var(--space-3); margin-bottom: var(--space-3);">
          <a href="#/runtime" style="color: var(--accent);">← Runtime</a>
          <a href="#/runtime" style="color: var(--accent);">Switch task</a>
        </nav>
        <header>
          <h1 style="margin-bottom: var(--space-2);">${projection?.task.objective ?? 'Runtime task'}</h1>
          <div role="status" style="color: var(--muted-foreground);">
            ${projection?.task.status ?? this.connection}
            ${this.connection === 'offline' ? ' · Offline' : ''}
            ${this.history ? ' · History' : ''}
          </div>
          <p style="color: ${pendingApprovals ? 'var(--error)' : 'var(--muted-foreground)'};">
            ${pendingApprovals ? `Approval required (${pendingApprovals})` : 'No intervention required'}
          </p>
          <slot name="task-actions"></slot>
        </header>
        ${this.error ? html`<p role="alert" style="color: var(--error);">${this.error}</p>` : ''}
        <details style="margin-top: var(--space-3);">
          <summary>Details and IDs</summary>
          <dl>
            <dt>Task ID</dt><dd>${this.taskId}</dd>
            <dt>Run ID</dt><dd>${projection?.task.currentRunId ?? 'None'}</dd>
            <dt>Session key</dt><dd>${projection?.task.sessionKey ?? 'None'}</dd>
          </dl>
        </details>
        ${this.renderProjection()}
      </article>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-runtime-task': RuntimeTaskView;
  }
}
