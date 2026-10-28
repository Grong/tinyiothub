import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { getRuntimeTask } from '../../api/runtime.js';
import { connectRuntimeStream } from '../../api/runtime-stream.js';
import type { PlaybackDescriptor, TaskProjection } from '../../types/runtime.js';
import { RuntimeProjection, type RuntimeConnection } from './runtime-projection.js';
import { toRuntimeTaskUrl, type RuntimeHistory, type RuntimeView } from './runtime-url-state.js';
import './runtime-playback.js';
import type { RuntimePlayback } from './runtime-playback.js';
import './runtime-loop.js';
import './runtime-devices.js';
import type { RuntimeDevices } from './runtime-devices.js';
import './runtime-flow.js';
import './runtime-task.css';

const minimumDeviceWidth = 360;
const collapsedDeviceWidth = 36;
const deviceWidthStep = 24;
const deviceWidthKey = 'runtime.devices.width';
const deviceExpandedKey = 'runtime.devices.expanded';

function sameHistory(a: RuntimeHistory | null, b: RuntimeHistory | null): boolean {
  return a?.at === b?.at && a?.event === b?.event;
}

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
  @state() private deviceWidth = 400;
  @state() private devicesExpanded = true;
  @state() private reducedMotion = false;

  private generation = 0;
  private abort: AbortController | null = null;
  private closeStream: (() => void) | null = null;
  private drag: { pointerId: number; startX: number; startWidth: number } | null = null;
  private motionQuery: MediaQueryList | null = null;
  private motionListener = (event: MediaQueryListEvent) => {
    this.reducedMotion = event.matches;
  };

  createRenderRoot() {
    return this;
  }

  connectedCallback(): void {
    super.connectedCallback();
    this.restoreLayout();
    if (typeof window.matchMedia === 'function') {
      this.motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
      this.reducedMotion = this.motionQuery.matches;
      this.motionQuery.addEventListener('change', this.motionListener);
    }
    if (this.taskId) void this.load();
  }

  disconnectedCallback(): void {
    this.cleanupTask();
    this.finishPointerDrag();
    this.motionQuery?.removeEventListener('change', this.motionListener);
    this.motionQuery = null;
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    const taskChanged = changed.has('taskId') && changed.get('taskId') !== undefined;
    const historyChanged = changed.has('history') && changed.get('history') !== undefined &&
      !sameHistory(changed.get('history') ?? null, this.history);
    if (taskChanged || historyChanged) {
      this.cleanupTask();
      if (this.taskId) void this.load();
    }
  }

  private restoreLayout(): void {
    const storedWidth = Number(localStorage.getItem(deviceWidthKey));
    if (Number.isFinite(storedWidth) && storedWidth > 0) this.deviceWidth = this.clampDeviceWidth(storedWidth);
    this.devicesExpanded = localStorage.getItem(deviceExpandedKey) !== 'false';
  }

  private get maximumDeviceWidth(): number {
    return Math.max(minimumDeviceWidth, window.innerWidth * 0.75);
  }

  private clampDeviceWidth(width: number): number {
    return Math.min(Math.max(width, minimumDeviceWidth), this.maximumDeviceWidth);
  }

  private setDeviceWidth(width: number): void {
    this.deviceWidth = this.clampDeviceWidth(width);
    localStorage.setItem(deviceWidthKey, String(this.deviceWidth));
  }

  private onSeamKey(event: KeyboardEvent): void {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    const target = event.target as HTMLElement | null;
    if (target?.matches('input, textarea, [contenteditable="true"]')) return;
    event.preventDefault();
    const delta = event.key === 'ArrowLeft' ? -deviceWidthStep : deviceWidthStep;
    this.setDeviceWidth(this.deviceWidth + delta);
  }

  private onPointerDown(event: PointerEvent): void {
    if (event.button !== 0 || !this.devicesExpanded) return;
    event.preventDefault();
    this.drag = { pointerId: event.pointerId, startX: event.clientX, startWidth: this.deviceWidth };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  private onPointerMove(event: PointerEvent): void {
    if (!this.drag || this.drag.pointerId !== event.pointerId) return;
    this.setDeviceWidth(this.drag.startWidth - (event.clientX - this.drag.startX));
  }

  private onPointerEnd(event: PointerEvent): void {
    if (!this.drag || this.drag.pointerId !== event.pointerId) return;
    const seam = event.currentTarget as HTMLElement;
    this.drag = null;
    if (typeof seam.hasPointerCapture !== 'function' || seam.hasPointerCapture(event.pointerId)) {
      seam.releasePointerCapture(event.pointerId);
    }
  }

  private finishPointerDrag(): void {
    if (!this.drag) return;
    const seam = this.querySelector<HTMLElement>('[role="separator"]');
    if (seam && (typeof seam.hasPointerCapture !== 'function' || seam.hasPointerCapture(this.drag.pointerId))) {
      seam.releasePointerCapture(this.drag.pointerId);
    }
    this.drag = null;
  }

  private toggleDevices(): void {
    this.devicesExpanded = !this.devicesExpanded;
    localStorage.setItem(deviceExpandedKey, String(this.devicesExpanded));
  }

  private syncUrl(): void {
    window.history.pushState({}, '', toRuntimeTaskUrl(this.taskId, {
      view: this.view,
      device: this.device,
      at: this.history?.at,
      event: this.history?.event,
    }));
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  }

  private changeView(view: RuntimeView): void {
    if (view === this.view) return;
    this.view = view;
    this.syncUrl();
    this.dispatchEvent(new CustomEvent('view-change', {
      detail: { view }, bubbles: true, composed: true,
    }));
  }

  private async locateDevice(event: CustomEvent<{ deviceId: string }>): Promise<void> {
    event.stopPropagation();
    this.device = event.detail.deviceId;
    if (!this.devicesExpanded) {
      this.devicesExpanded = true;
      localStorage.setItem(deviceExpandedKey, 'true');
    }
    this.syncUrl();
    await this.updateComplete;
    this.querySelector<RuntimeDevices>('runtime-devices')?.focusDevice(this.device, this.reducedMotion);
  }

  private selectHistoryEvent(event: CustomEvent<{ eventId: string }>): void {
    this.history = { ...this.history, event: event.detail.eventId };
    this.syncUrl();
  }

  private cleanupTask(): void {
    this.generation += 1;
    this.closeStream?.();
    this.closeStream = null;
    this.abort?.abort();
    this.abort = null;
    this.querySelector<RuntimePlayback>('runtime-playback')?.destroy();
    this.descriptor = undefined;
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

  private async refreshProjection(): Promise<void> {
    const generation = this.generation;
    const taskId = this.taskId;
    this.abort?.abort();
    this.abort = new AbortController();
    this.error = '';
    try {
      const response = await getRuntimeTask(this.workspaceId, taskId, this.abort.signal);
      if (generation !== this.generation || taskId !== this.taskId) return;
      if (!response.result) throw new ApiError(500, 'Task response missing');
      this.installSnapshot(response.result, response.result.cursor);
    } catch (error) {
      if (generation !== this.generation || this.abort?.signal.aborted) return;
      this.error = error instanceof ApiError ? error.message : 'Network error refreshing Runtime task';
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
    void this.revision;
    const currentWidth = this.devicesExpanded ? this.deviceWidth : collapsedDeviceWidth;
    return html`
      ${this.descriptor
        ? html`<runtime-playback .descriptor=${this.descriptor}></runtime-playback>`
        : nothing}
      <section class="runtime-workbench">
        <div class="runtime-workbench__main">
          <div class="runtime-tabs" role="tablist" aria-label="Runtime task views">
            <button type="button" role="tab" data-view="loop" aria-selected=${this.view === 'loop'}
              @click=${() => this.changeView('loop')}>Agent Loop</button>
            <button type="button" role="tab" data-view="flow" aria-selected=${this.view === 'flow'}
              @click=${() => this.changeView('flow')}>Flow</button>
          </div>
          <div role="tabpanel" class="runtime-workbench__panel">
            ${this.view === 'flow'
              ? html`<runtime-flow .projection=${projection} .revision=${this.revision} .reducedMotion=${this.reducedMotion}
                  @locate-device=${this.locateDevice} @event-select=${this.selectHistoryEvent}></runtime-flow>`
              : html`<runtime-loop .projection=${projection} .revision=${this.revision}
                  @locate-device=${this.locateDevice} @refresh-request=${this.refreshProjection}></runtime-loop>`}
          </div>
        </div>
        <div class="runtime-seam">
          <div class="runtime-seam__handle" role="separator" tabindex=${this.devicesExpanded ? '0' : '-1'}
            aria-label="Resize associated devices. Use Left and Right arrow keys in 24 pixel steps."
            aria-orientation="vertical" aria-valuenow=${currentWidth}
            aria-valuemin=${this.devicesExpanded ? minimumDeviceWidth : collapsedDeviceWidth}
            aria-valuemax=${this.maximumDeviceWidth}
            aria-valuetext=${this.devicesExpanded ? `${currentWidth} pixels` : 'Devices collapsed, 36 pixels'}
            @keydown=${this.onSeamKey} @pointerdown=${this.onPointerDown} @pointermove=${this.onPointerMove}
            @pointerup=${this.onPointerEnd} @pointercancel=${this.onPointerEnd}></div>
          <button type="button" class="runtime-seam__toggle" aria-controls="runtime-devices-panel"
            aria-expanded=${this.devicesExpanded} aria-label=${this.devicesExpanded ? 'Collapse devices' : 'Expand devices'}
            @click=${this.toggleDevices}>${this.devicesExpanded ? '›' : '‹'}</button>
        </div>
        <aside id="runtime-devices-panel" class="runtime-devices-panel ${this.devicesExpanded ? '' : 'is-collapsed'}"
          style="width: ${currentWidth}px;">
          <runtime-devices .projection=${projection} .revision=${this.revision} .selectedDevice=${this.device}
            ?hidden=${!this.devicesExpanded} @locate-device=${this.locateDevice}></runtime-devices>
          ${this.devicesExpanded ? nothing : html`<div class="runtime-devices-rail"><span>Devices</span></div>`}
        </aside>
      </section>
    `;
  }

  render() {
    const projection = this.projection;
    const pendingApprovals = projection?.approvals.filter((approval) => approval.state === 'pending').length ?? 0;
    return html`
      <article class="runtime-task" aria-busy=${projection ? 'false' : 'true'}>
        <nav class="runtime-task__navigation" aria-label="Runtime task navigation">
          <a href="#/runtime">← Runtime</a>
          <a href="#/runtime">Switch task</a>
        </nav>
        <header>
          <h1>${projection?.task.objective ?? 'Runtime task'}</h1>
          <div role="status" class="runtime-task__status">
            ${projection?.task.status ?? this.connection}
            ${this.connection === 'offline' ? ' · Offline' : ''}
            ${this.history ? ' · History' : ''}
          </div>
          <p class="runtime-task__intervention ${pendingApprovals ? 'has-approval' : ''}">
            ${pendingApprovals ? `Approval required (${pendingApprovals})` : 'No intervention required'}
          </p>
          <slot name="task-actions"></slot>
        </header>
        ${this.error ? html`<p role="alert" class="runtime-task__error">${this.error}</p>` : ''}
        <details class="runtime-task__details">
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
