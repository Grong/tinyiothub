import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { getPlaybackDescriptor } from '../../api/playback.js';
import { getRuntimeTask, listRuntimeTasks } from '../../api/runtime.js';
import { connectRuntimeStream } from '../../api/runtime-stream.js';
import type {
  DeviceJobState,
  PlaybackDescriptor,
  RuntimeEvent,
  RuntimeTask,
  TaskProjection,
  TaskStatus,
} from '../../types/runtime.js';
import { RuntimeProjection, type RuntimeConnection } from './runtime-projection.js';
import {
  isHistoryState,
  switchTaskUrl,
  toLiveRuntimeTaskUrl,
  toRuntimeTaskUrl,
  type RuntimeHistory,
  type RuntimeView,
} from './runtime-url-state.js';
import './runtime-task-controls.js';
import './runtime-playback.js';
import type { RuntimePlayback } from './runtime-playback.js';
import './runtime-loop.js';
import './runtime-devices.js';
import type { RuntimeDevices } from './runtime-devices.js';
import './runtime-flow.js';
import './runtime-task.css';
import './runtime-workbench.css';

const minimumDeviceWidth = 360;
const collapsedDeviceWidth = 36;
const deviceWidthStep = 24;
const deviceWidthKey = 'runtime.devices.width';
const deviceExpandedKey = 'runtime.devices.expanded';

function sameHistory(a: RuntimeHistory | null, b: RuntimeHistory | null): boolean {
  return a?.at === b?.at && a?.event === b?.event;
}

function statusLabel(status: string): string {
  return status.replaceAll('_', ' ');
}

type RuntimePanel = RuntimeView | 'devices';

const runtimePanels: RuntimePanel[] = ['loop', 'flow', 'devices'];

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
  @state() private announcement = '';
  @state() private mobilePanel: RuntimePanel | null = null;
  @state() private mobile = false;
  @state() private playbackError = '';
  @state() private playbackLoading = false;
  @state() private switchOpen = false;
  @state() private switchTasks: RuntimeTask[] = [];
  @state() private switchLoading = false;
  @state() private switchError = '';

  private generation = 0;
  private abort: AbortController | null = null;
  private playbackAbort: AbortController | null = null;
  private switchAbort: AbortController | null = null;
  private closeStream: (() => void) | null = null;
  private historyEventId = '';
  private historyOccurredAt = '';
  private drag: { pointerId: number; startX: number; startWidth: number } | null = null;
  private motionQuery: MediaQueryList | null = null;
  private mobileQuery: MediaQueryList | null = null;
  private lastTaskStatus: TaskStatus | null = null;
  private lastJobStates = new Map<string, DeviceJobState>();
  private pendingObjectiveFocus = false;
  private motionListener = (event: MediaQueryListEvent) => {
    this.reducedMotion = event.matches;
  };
  private mobileListener = (event: MediaQueryListEvent) => {
    this.mobile = event.matches;
    if (!event.matches) this.mobilePanel = null;
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
      this.mobileQuery = window.matchMedia('(max-width: 599px)');
      this.mobile = this.mobileQuery.matches;
      this.mobileQuery.addEventListener('change', this.mobileListener);
    }
    if (this.taskId) void this.load();
  }

  disconnectedCallback(): void {
    this.cleanupTask();
    this.finishPointerDrag();
    this.motionQuery?.removeEventListener('change', this.motionListener);
    this.motionQuery = null;
    this.mobileQuery?.removeEventListener('change', this.mobileListener);
    this.mobileQuery = null;
    super.disconnectedCallback();
  }

  protected updated(): void {
    if (!this.pendingObjectiveFocus || !this.projection) return;
    this.pendingObjectiveFocus = false;
    this.querySelector<HTMLElement>('h1')?.focus({ preventScroll: true });
  }

  willUpdate(changed: PropertyValues<this>): void {
    const taskChanged = changed.has('taskId') && changed.get('taskId') !== undefined;
    const historyChanged = changed.has('history') && changed.get('history') !== undefined &&
      !sameHistory(changed.get('history') ?? null, this.history);
    if (taskChanged) {
      const previousTaskId = changed.get('taskId');
      if (typeof previousTaskId === 'string' && previousTaskId && previousTaskId !== this.taskId) {
        this.pendingObjectiveFocus = true;
      }
      this.historyEventId = '';
      this.historyOccurredAt = '';
      this.mobilePanel = null;
      this.announcement = '';
      this.lastTaskStatus = null;
      this.lastJobStates = new Map();
    }
    if (taskChanged) {
      this.cleanupTask();
      if (this.taskId) void this.load();
    } else if (historyChanged) {
      if (this.historyMode) {
        const steps = this.projection?.steps ?? [];
        this.cleanupTask();
        if (this.taskId) void this.loadHistory(steps);
      } else {
        this.cleanupTask();
        if (this.taskId) void this.load();
      }
    }
    if (this.projection) this.syncAnnouncements();
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
    // The devices panel sits on the right, so moving the seam left widens it.
    const delta = event.key === 'ArrowLeft' ? deviceWidthStep : -deviceWidthStep;
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

  private get activePanel(): RuntimePanel {
    return this.mobilePanel ?? this.view;
  }

  private get visiblePanels(): RuntimePanel[] {
    return this.mobile ? runtimePanels : runtimePanels.filter((panel) => panel !== 'devices');
  }

  private get objectiveLabel(): string {
    return this.projection?.task.objective ?? 'Runtime task';
  }

  private selectPanel(panel: RuntimePanel, moveFocus = true): void {
    if (panel === 'devices' && !this.mobile) return;
    if (panel === 'devices') {
      this.mobilePanel = 'devices';
    } else {
      this.mobilePanel = null;
      this.changeView(panel);
    }
    if (!moveFocus) return;
    void this.updateComplete.then(() => {
      this.querySelector<HTMLButtonElement>(`[role="tab"][data-view="${panel}"]`)?.focus();
    });
  }

  private onTabKey(event: KeyboardEvent): void {
    const panels = this.visiblePanels;
    const current = panels.indexOf(this.activePanel);
    let next = current < 0 ? 0 : current;
    if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = panels.length - 1;
    else if (event.key === 'ArrowLeft') next = (next - 1 + panels.length) % panels.length;
    else if (event.key === 'ArrowRight') next = (next + 1) % panels.length;
    else return;
    event.preventDefault();
    this.selectPanel(panels[next]);
  }

  private jumpToDevices(): void {
    const panel = this.querySelector<HTMLElement>('#runtime-devices-panel');
    if (!panel) return;
    panel.scrollIntoView({
      behavior: this.reducedMotion ? 'auto' : 'smooth',
      block: 'start',
    });
    const heading = panel.querySelector<HTMLElement>('h2');
    if (!heading) return;
    heading.tabIndex = -1;
    heading.focus({ preventScroll: true });
  }

  private playbackAlt(): string {
    const devices = this.projection?.devices ?? [];
    const device = devices.find((item) => item.deviceId === this.device) ??
      devices.find((item) => item.sources?.some((source) => source.dataKind === 'video')) ??
      devices[0];
    const source = device?.sources?.find((item) => item.dataKind === 'video');
    if (device && source) return `Live camera frame from ${device.deviceId}, source ${source.id}`;
    if (device) return `Live camera frame from ${device.deviceId}`;
    return 'Live camera frame';
  }

  private get playbackTarget(): { deviceId: string; sourceId: string } | null {
    const devices = this.projection?.devices ?? [];
    const ordered = this.device
      ? [...devices.filter((item) => item.deviceId === this.device),
          ...devices.filter((item) => item.deviceId !== this.device)]
      : devices;
    for (const device of ordered) {
      const source = device.sources?.find((item) => item.dataKind === 'video');
      if (source) return { deviceId: device.deviceId, sourceId: source.id };
    }
    return null;
  }

  private async playEvidence(): Promise<void> {
    const target = this.playbackTarget;
    if (!target) {
      this.playbackError = 'playback_unavailable';
      return;
    }
    if (this.playbackLoading) return;
    const generation = this.generation;
    this.playbackAbort?.abort();
    const controller = new AbortController();
    this.playbackAbort = controller;
    this.playbackLoading = true;
    this.playbackError = '';
    this.descriptor = undefined;
    try {
      const descriptor = await getPlaybackDescriptor(
        this.workspaceId, target.deviceId, target.sourceId, controller.signal,
      );
      if (generation !== this.generation) return;
      const expiresAt = Date.parse(descriptor.expiresAt);
      if ((descriptor.protocol !== 'hls' && descriptor.protocol !== 'mjpeg') ||
        !Number.isFinite(expiresAt) || expiresAt <= Date.now()) {
        this.playbackError = 'playback_unavailable';
        return;
      }
      this.descriptor = descriptor;
    } catch {
      if (generation !== this.generation || controller.signal.aborted) return;
      this.playbackError = 'playback_unavailable';
    } finally {
      if (generation === this.generation) this.playbackLoading = false;
    }
  }

  private async openSwitchTask(): Promise<void> {
    this.switchOpen = true;
    this.switchError = '';
    this.switchAbort?.abort();
    const controller = new AbortController();
    this.switchAbort = controller;
    this.switchLoading = true;
    try {
      const response = await listRuntimeTasks(this.workspaceId, 50, 0, controller.signal);
      if (controller.signal.aborted) return;
      this.switchTasks = response.result?.tasks ?? [];
    } catch {
      if (controller.signal.aborted) return;
      this.switchError = 'Unable to load Runtime tasks';
    } finally {
      if (!controller.signal.aborted) this.switchLoading = false;
    }
    await this.updateComplete;
    this.querySelector<HTMLElement>('[role="option"]')?.focus();
  }

  private closeSwitchTask(): void {
    this.switchOpen = false;
    void this.updateComplete.then(() => {
      this.querySelector<HTMLButtonElement>('[data-action="switch-task"]')?.focus();
    });
  }

  private chooseTask(taskId: string): void {
    if (!taskId || taskId === this.taskId) {
      this.closeSwitchTask();
      return;
    }
    this.switchOpen = false;
    window.history.pushState({}, '', switchTaskUrl(taskId, this.view));
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  }

  private onSwitchKey(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      this.closeSwitchTask();
    }
  }

  private syncAnnouncements(): void {
    const projection = this.projection;
    if (!projection) return;
    const messages: string[] = [];
    const previousTask = this.lastTaskStatus;
    const nextTask = projection.task.status;
    if (previousTask !== null && previousTask !== nextTask) {
      messages.push(`Task ${statusLabel(nextTask)}`);
    }
    this.lastTaskStatus = nextTask;

    const nextJobs = new Map<string, DeviceJobState>();
    for (const job of projection.jobs) {
      nextJobs.set(job.id, job.state);
      const previous = this.lastJobStates.get(job.id);
      if (previous !== undefined && previous !== job.state) {
        messages.push(`Job ${job.id} ${statusLabel(job.state)}`);
      } else if (previous === undefined && previousTask !== null &&
        (job.state === 'accepted' || job.state === 'running')) {
        messages.push(`Job ${job.id} ${statusLabel(job.state)}`);
      }
    }
    this.lastJobStates = nextJobs;
    if (messages.length) this.announcement = messages.join('. ');
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
    const eventId = event.detail.eventId;
    this.historyEventId = eventId;
    this.historyOccurredAt = this.projection?.steps.find((step) => step.eventId === eventId)?.occurredAt ?? '';
    this.history = { ...this.history, event: eventId };
    this.syncUrl();
  }

  private get historyMode(): boolean {
    return isHistoryState(this.history);
  }

  private historyTime(): string {
    if (this.history?.at) return this.history.at;
    if (this.history?.event) {
      const fromProjection = this.projection?.steps
        .find((step) => step.eventId === this.history?.event)?.occurredAt;
      if (fromProjection) return fromProjection;
      if (this.historyEventId === this.history.event) return this.historyOccurredAt;
    }
    return '';
  }

  private backToLive(): void {
    if (!this.historyMode) return;
    this.history = null;
    this.historyEventId = '';
    this.historyOccurredAt = '';
    window.history.replaceState({}, '', toLiveRuntimeTaskUrl(this.taskId, {
      view: this.view,
      device: this.device,
    }));
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  }

  private cleanupTask(): void {
    this.generation += 1;
    this.closeStream?.();
    this.closeStream = null;
    this.abort?.abort();
    this.abort = null;
    this.playbackAbort?.abort();
    this.playbackAbort = null;
    this.switchAbort?.abort();
    this.switchAbort = null;
    this.querySelector<RuntimePlayback>('runtime-playback')?.destroy();
    this.descriptor = undefined;
    this.playbackError = '';
    this.playbackLoading = false;
    this.switchOpen = false;
    this.switchTasks = [];
    this.switchLoading = false;
    this.switchError = '';
    this.projection = null;
  }

  private onProjectionChange(event: CustomEvent<TaskProjection>): void {
    event.stopPropagation();
    const snapshot = event.detail;
    this.installSnapshot(snapshot, snapshot.cursor);
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
      if (!this.historyMode) this.openStream(generation, taskId);
    } catch (error) {
      if (generation !== this.generation || this.abort?.signal.aborted) return;
      this.connection = 'offline';
      this.error = error instanceof ApiError ? error.message : 'Network error loading Runtime task';
    }
  }

  private async loadHistory(steps: RuntimeEvent[]): Promise<void> {
    const generation = ++this.generation;
    const taskId = this.taskId;
    this.abort = new AbortController();
    this.error = '';
    try {
      const response = await getRuntimeTask(this.workspaceId, taskId, this.abort.signal);
      if (generation !== this.generation || taskId !== this.taskId) return;
      if (!response.result) throw new ApiError(500, 'Task response missing');
      const projection = new RuntimeProjection(response.result, { history: true });
      projection.restoreHistorySteps(steps);
      this.projection = projection;
      this.revision += 1;
    } catch (error) {
      if (generation !== this.generation || this.abort?.signal.aborted) return;
      this.error = error instanceof ApiError ? error.message : 'Network error loading Runtime history';
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
    else this.projection = new RuntimeProjection(snapshot, { history: this.historyMode });
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
      ${this.playbackError
        ? html`<p class="runtime-task__error" role="status" data-playback-error>${this.playbackError}</p>`
        : nothing}
      ${this.descriptor
        ? html`<runtime-playback .descriptor=${this.descriptor} .alt=${this.playbackAlt()}></runtime-playback>`
        : nothing}
      <section class="runtime-workbench" data-view=${this.activePanel}
        style=${`--runtime-device-width: ${currentWidth}px`}>
        <div class="runtime-workbench__main">
          <div class="runtime-view-tabs-row">
            <div class="runtime-view-tabs" role="tablist" aria-label="Runtime view" @keydown=${this.onTabKey}>
              <button type="button" role="tab" data-view="loop" id="runtime-tab-loop"
                aria-controls="runtime-panel" aria-selected=${this.activePanel === 'loop'}
                tabindex=${this.activePanel === 'loop' ? '0' : '-1'}
                @click=${() => this.selectPanel('loop', false)}>Agent Loop</button>
              <button type="button" role="tab" data-view="flow" id="runtime-tab-flow"
                aria-controls="runtime-panel" aria-selected=${this.activePanel === 'flow'}
                tabindex=${this.activePanel === 'flow' ? '0' : '-1'}
                @click=${() => this.selectPanel('flow', false)}>Flow</button>
              ${this.mobile
                ? html`<button type="button" role="tab" data-view="devices" id="runtime-tab-devices"
                    aria-controls="runtime-devices-panel" aria-selected=${this.activePanel === 'devices'}
                    tabindex=${this.activePanel === 'devices' ? '0' : '-1'}
                    @click=${() => this.selectPanel('devices', false)}>Devices</button>`
                : nothing}
            </div>
            <button type="button" class="runtime-task__devices-anchor" @click=${this.jumpToDevices}>Devices</button>
          </div>
          <div role="tabpanel" id="runtime-panel" class="runtime-workbench__panel"
            aria-labelledby=${`runtime-tab-${this.view}`}>
            ${this.view === 'flow'
              ? html`<runtime-flow .projection=${projection} .revision=${this.revision} .reducedMotion=${this.reducedMotion}
                  @locate-device=${this.locateDevice} @event-select=${this.selectHistoryEvent}></runtime-flow>`
              : html`<runtime-loop .projection=${projection} .revision=${this.revision}
                  .readOnly=${this.historyMode}
                  @locate-device=${this.locateDevice} @refresh-request=${this.refreshProjection}
                  @projection-change=${this.onProjectionChange}></runtime-loop>`}
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

  private renderHistoryBanner() {
    if (!this.historyMode) return nothing;
    const time = this.historyTime();
    return html`<section class="runtime-task__history" data-history-banner
      aria-labelledby="runtime-history-title">
      <strong id="runtime-history-title">History · read-only</strong>
      ${time ? html`<span> · ${time}</span>` : nothing}
      <button type="button" data-action="back-to-live" @click=${this.backToLive}>Back to live</button>
    </section>`;
  }

  private renderConnectionBanner() {
    if (this.connection !== 'offline') return nothing;
    return html`<p class="runtime-task__connection" data-connection="offline" role="status">
      <strong>Offline</strong> · showing the last known state. Live updates resume when the connection returns.
    </p>`;
  }

  render() {
    const projection = this.projection;
    const pendingApprovals = projection?.approvals.filter((approval) => approval.state === 'pending').length ?? 0;
    return html`
      <article class="runtime-task" aria-busy=${projection ? 'false' : 'true'}>
        <nav class="runtime-task__navigation" aria-label="Runtime task navigation">
          <a href="#/runtime">← Runtime</a>
          <button type="button" class="runtime-task__switch" data-action="switch-task"
            @click=${this.openSwitchTask}>Switch task</button>
        </nav>
        <div class="runtime-announcer" data-announcer aria-live="polite" aria-atomic="true"
          >${this.announcement}</div>
        <header class="runtime-task-header">
          <h1 class="runtime-objective" tabindex="-1">${this.objectiveLabel}</h1>
          <div role="status" class="runtime-task__status">
            ${projection?.task.status ?? this.connection}
          </div>
          <p class="runtime-task__intervention ${pendingApprovals ? 'has-approval' : ''}">
            ${pendingApprovals ? `Approval required (${pendingApprovals})` : 'No intervention required'}
          </p>
          ${projection
            ? html`<runtime-task-controls .workspaceId=${this.workspaceId} .taskId=${this.taskId}
                .projection=${projection} .revision=${this.revision} .readOnly=${this.historyMode}
                @projection-change=${this.onProjectionChange}
                @refresh-request=${this.refreshProjection}></runtime-task-controls>`
            : nothing}
          ${projection && this.playbackTarget && !this.historyMode
            ? html`<div class="runtime-task__playback-actions">
                <button type="button" data-action="play-evidence" ?disabled=${this.playbackLoading}
                  @click=${this.playEvidence}>${this.playbackLoading ? 'Loading evidence…' : 'Play evidence'}</button>
              </div>`
            : nothing}
          <slot name="task-actions"></slot>
        </header>
        ${this.renderHistoryBanner()}
        ${this.renderConnectionBanner()}
        ${this.error ? html`<p role="alert" class="runtime-task__error">${this.error}</p>` : ''}
        <details class="runtime-task__details">
          <summary>Details and IDs</summary>
          <dl>
            <dt>Task ID</dt><dd>${this.taskId}</dd>
            <dt>Run ID</dt><dd>${projection?.task.currentRunId ?? 'None'}</dd>
            <dt>Session key</dt><dd>${projection?.task.sessionKey ?? 'None'}</dd>
          </dl>
        </details>
        ${projection
          ? this.renderProjection()
          : html`<div class="runtime-loading" data-loading aria-hidden="true">
              <span></span><span></span><span></span>
            </div>`}
        ${this.switchOpen
          ? html`<div class="runtime-task__switch-overlay" role="dialog" aria-modal="true"
              aria-labelledby="runtime-switch-title" @keydown=${this.onSwitchKey}>
              <section class="runtime-task__switch-dialog">
                <h2 id="runtime-switch-title">Switch task</h2>
                ${this.switchLoading ? html`<p role="status">Loading tasks…</p>` : nothing}
                ${this.switchError ? html`<p role="alert">${this.switchError}</p>` : nothing}
                <div role="listbox" aria-label="Runtime tasks">
                  ${this.switchTasks.map((task) => html`
                    <button type="button" role="option" aria-label=${task.id}
                      aria-selected=${task.id === this.taskId} @click=${() => this.chooseTask(task.id)}>
                      <strong>${task.objective}</strong>
                      <span>${task.id} · ${statusLabel(task.status)}</span>
                    </button>`)}
                </div>
                ${!this.switchLoading && !this.switchError && this.switchTasks.length === 0
                  ? html`<p>No other Runtime tasks</p>`
                  : nothing}
                <button type="button" data-action="keep-task" @click=${this.closeSwitchTask}>Keep current task</button>
              </section>
            </div>`
          : nothing}
      </article>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-runtime-task': RuntimeTaskView;
  }
}
