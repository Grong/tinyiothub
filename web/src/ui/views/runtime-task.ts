import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError, getWorkspaceId } from '../../api/client.js';
import { getPlaybackDescriptor, isSafePlaybackUrl } from '../../api/playback.js';
import { getRuntimeTask, listRuntimeTaskEvents, listRuntimeTasks } from '../../api/runtime.js';
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
import { createDialogFocusTrap, type DialogFocusTrap } from './runtime-dialog-focus.js';
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
const collapsedDeviceWidth = 44;
const deviceWidthStep = 24;
const deviceWidthKey = 'runtime.devices.width';
const deviceExpandedKey = 'runtime.devices.expanded';
const timelinePageSize = 100;
const timelineSeedTimeoutMs = 10_000;
const playbackRefreshLeadMs = 30_000;
const playbackRefreshRetryMs = 10_000;

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
  @state() private timelineLoading = false;
  @state() private timelineError = '';
  @state() private timelineHasMore = false;
  @state() private timelineOldest: number | null = null;
  @state() private historyRejected = false;
  @state() private historyNotice = '';

  private generation = 0;
  private coldHistoryStripped = false;
  private abort: AbortController | null = null;
  private playbackAbort: AbortController | null = null;
  private timelineAbort: AbortController | null = null;
  private descriptorTarget: { deviceId: string; sourceId: string } | null = null;
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
  private playbackRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  private switchTrap: DialogFocusTrap | null = null;
  private switchTrapTarget: HTMLElement | null = null;
  private motionListener = (event: MediaQueryListEvent) => {
    this.reducedMotion = event.matches;
  };
  private mobileListener = (event: MediaQueryListEvent) => {
    this.mobile = event.matches;
    if (event.matches) {
      // The collapsed rail has no seam toggle below 980px; never trap mobile
      // users behind a persisted collapsed state.
      this.setDevicesExpanded(true);
    } else {
      this.mobilePanel = null;
    }
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
      if (this.mobile) this.setDevicesExpanded(true, false);
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
    this.syncSwitchTrap();
    if (!this.pendingObjectiveFocus || !this.projection) return;
    this.pendingObjectiveFocus = false;
    this.querySelector<HTMLElement>('h1')?.focus({ preventScroll: true });
  }

  willUpdate(changed: PropertyValues<this>): void {
    // Any device change (including one driven by the URL) invalidates the
    // current signed playback session; never let A's media survive into B.
    if (changed.has('device') && changed.get('device') !== this.device) this.stopPlayback();
    const taskChanged = changed.has('taskId') && changed.get('taskId') !== undefined;
    const historyChanged =
      changed.has('history') &&
      changed.get('history') !== undefined &&
      !sameHistory(changed.get('history') ?? null, this.history);
    const coldHistoryStripped = this.coldHistoryStripped;
    // The router clearing the stale history value after a cold-link recovery is
    // not a user navigation: keep the rejection (and its one-time notice) so the
    // live view underneath the notice is not reloaded and the notice survives.
    const clearingColdHistory = coldHistoryStripped && !isHistoryState(this.history);
    if (historyChanged && !clearingColdHistory) {
      // An explicit navigation supersedes a previous cold-link rejection.
      this.historyRejected = false;
      this.coldHistoryStripped = false;
      this.historyNotice = '';
    }
    if (historyChanged && !this.historyMode) {
      // Leaving history by any route (Back to live, browser Back/Forward, URL
      // edit) invalidates the in-page event selection; a stale selection must
      // never re-enable freezing a projection with no steps.
      this.historyEventId = '';
      this.historyOccurredAt = '';
    }
    if (taskChanged) {
      const previousTaskId = changed.get('taskId');
      if (typeof previousTaskId === 'string' && previousTaskId && previousTaskId !== this.taskId) {
        this.pendingObjectiveFocus = true;
      }
      this.historyEventId = '';
      this.historyOccurredAt = '';
      this.historyRejected = false;
      this.coldHistoryStripped = false;
      this.historyNotice = '';
      this.mobilePanel = null;
      this.announcement = '';
      this.lastTaskStatus = null;
      this.lastJobStates = new Map();
    }
    if (taskChanged) {
      this.cleanupTask();
      if (this.taskId) void this.load();
    } else if (historyChanged) {
      if (coldHistoryStripped && !this.historyMode) {
        // The cold-link recovery already reloaded the live view in this cycle;
        // the router clearing the stale history value must not reload it again.
      } else if (this.historyMode) {
        const steps = this.projection?.steps ?? [];
        if (!steps.length && !this.hasHistorySelection(steps)) {
          // No in-memory history to freeze (e.g. same-Task history navigation
          // that arrived before the initial snapshot installed). Fail closed to
          // live rather than render the live snapshot under a past banner.
          this.cleanupTask();
          if (this.taskId) void this.load();
        } else {
          this.cleanupTask();
          if (this.taskId) void this.loadHistory(steps);
        }
      } else {
        this.cleanupTask();
        if (this.taskId) void this.load();
      }
    }
    if (this.projection) this.syncAnnouncements();
  }

  private restoreLayout(): void {
    const storedWidth = Number(localStorage.getItem(deviceWidthKey));
    if (Number.isFinite(storedWidth) && storedWidth > 0)
      this.deviceWidth = this.clampDeviceWidth(storedWidth);
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
    if (
      seam &&
      (typeof seam.hasPointerCapture !== 'function' || seam.hasPointerCapture(this.drag.pointerId))
    ) {
      seam.releasePointerCapture(this.drag.pointerId);
    }
    this.drag = null;
  }

  private setDevicesExpanded(expanded: boolean, persist = true): void {
    this.devicesExpanded = expanded;
    if (persist) localStorage.setItem(deviceExpandedKey, String(expanded));
  }

  private toggleDevices(): void {
    this.setDevicesExpanded(!this.devicesExpanded);
  }

  private syncUrl(): void {
    window.history.pushState(
      {},
      '',
      toRuntimeTaskUrl(this.taskId, {
        view: this.view,
        device: this.device,
        at: this.history?.at,
        event: this.history?.event,
      }),
    );
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  }

  private changeView(view: RuntimeView): void {
    if (view === this.view) return;
    this.view = view;
    this.syncUrl();
    this.dispatchEvent(
      new CustomEvent('view-change', {
        detail: { view },
        bubbles: true,
        composed: true,
      }),
    );
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
      this.setDevicesExpanded(true);
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

  private async jumpToDevices(): Promise<void> {
    // A persisted collapsed rail used to be a dead end at tablet widths: the
    // seam toggle is hidden there, so the anchor must expand the panel first.
    this.setDevicesExpanded(true);
    await this.updateComplete;
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

  /**
   * Single resolver for playback target and alt text. An explicitly selected
   * device without a video source fails closed instead of silently playing a
   * different camera whose frame would then be misattributed in the alt text.
   */
  private resolvePlaybackTarget(): { deviceId: string; sourceId: string } | null {
    const devices = this.projection?.devices ?? [];
    const candidates = this.device
      ? devices.filter((item) => item.deviceId === this.device)
      : devices;
    for (const device of candidates) {
      const source = device.sources?.find((item) => item.dataKind === 'video');
      if (source) return { deviceId: device.deviceId, sourceId: source.id };
    }
    return null;
  }

  private playbackAlt(): string {
    // Once a descriptor is accepted, the alt must describe the target that
    // produced it, not whatever the projection resolves to on this render. A
    // descriptor supplied without a stored target (public property assignment)
    // falls back to the resolver, same as the no-descriptor case.
    const target = this.descriptor
      ? (this.descriptorTarget ?? this.resolvePlaybackTarget())
      : this.resolvePlaybackTarget();
    if (!target) return 'Live camera frame';
    return `Live camera frame from ${target.deviceId}, source ${target.sourceId}`;
  }

  private get playbackTarget(): { deviceId: string; sourceId: string } | null {
    return this.resolvePlaybackTarget();
  }

  /** Tear down media, signed URLs and timers; safe to call repeatedly. */
  private stopPlayback(): void {
    this.playbackAbort?.abort();
    this.playbackAbort = null;
    this.clearPlaybackRefresh();
    this.querySelector<RuntimePlayback>('runtime-playback')?.destroy();
    this.descriptor = undefined;
    this.descriptorTarget = null;
    this.playbackError = '';
    this.playbackLoading = false;
  }

  private usableDescriptor(descriptor: PlaybackDescriptor): boolean {
    if (descriptor.protocol !== 'hls' && descriptor.protocol !== 'mjpeg') return false;
    if (!isSafePlaybackUrl(descriptor.url)) return false;
    const expiresAt = Date.parse(descriptor.expiresAt);
    return Number.isFinite(expiresAt) && expiresAt > Date.now();
  }

  private clearPlaybackRefresh(): void {
    if (this.playbackRefreshTimer !== null) clearTimeout(this.playbackRefreshTimer);
    this.playbackRefreshTimer = null;
  }

  private schedulePlaybackRefresh(target: { deviceId: string; sourceId: string }): void {
    this.clearPlaybackRefresh();
    const descriptor = this.descriptor;
    if (!descriptor) return;
    const expiresAt = Date.parse(descriptor.expiresAt);
    if (!Number.isFinite(expiresAt)) return;
    const delay = Math.max(1_000, expiresAt - Date.now() - playbackRefreshLeadMs);
    // setTimeout clamps delays above 2^31-1 to ~1ms; a far-future descriptor
    // must not turn into a refresh loop.
    if (delay > 2_147_000_000) return;
    this.playbackRefreshTimer = setTimeout(() => {
      this.playbackRefreshTimer = null;
      void this.refreshPlaybackDescriptor(target, expiresAt);
    }, delay);
  }

  private async refreshPlaybackDescriptor(
    target: { deviceId: string; sourceId: string },
    previousExpiry: number,
  ): Promise<void> {
    const controller = this.playbackAbort;
    const current = this.descriptorTarget;
    if (
      !controller ||
      controller.signal.aborted ||
      !current ||
      current.deviceId !== target.deviceId ||
      current.sourceId !== target.sourceId
    )
      return;
    const generation = this.generation;
    try {
      const descriptor = await getPlaybackDescriptor(
        this.workspaceId,
        target.deviceId,
        target.sourceId,
        controller.signal,
      );
      if (generation !== this.generation || controller.signal.aborted) return;
      if (
        !this.descriptorTarget ||
        this.descriptorTarget.deviceId !== target.deviceId ||
        this.descriptorTarget.sourceId !== target.sourceId
      )
        return;
      if (!this.usableDescriptor(descriptor)) {
        this.failPlayback();
        return;
      }
      this.descriptor = descriptor;
      this.schedulePlaybackRefresh(target);
    } catch {
      if (generation !== this.generation || controller.signal.aborted) return;
      if (Date.now() >= previousExpiry) {
        this.failPlayback();
        return;
      }
      this.playbackRefreshTimer = setTimeout(() => {
        this.playbackRefreshTimer = null;
        void this.refreshPlaybackDescriptor(target, previousExpiry);
      }, playbackRefreshRetryMs);
    }
  }

  private failPlayback(): void {
    this.stopPlayback();
    this.playbackError = 'playback_unavailable';
  }

  private async playEvidence(): Promise<void> {
    const target = this.playbackTarget;
    if (!target) {
      this.failPlayback();
      return;
    }
    if (this.playbackLoading) return;
    const generation = this.generation;
    this.playbackAbort?.abort();
    this.clearPlaybackRefresh();
    const controller = new AbortController();
    this.playbackAbort = controller;
    this.playbackLoading = true;
    this.playbackError = '';
    this.descriptor = undefined;
    this.descriptorTarget = null;
    try {
      const descriptor = await getPlaybackDescriptor(
        this.workspaceId,
        target.deviceId,
        target.sourceId,
        controller.signal,
      );
      if (generation !== this.generation || controller.signal.aborted) return;
      // The projection may have moved to another source while the descriptor
      // was in flight; never attach a descriptor to a target it does not match.
      const current = this.resolvePlaybackTarget();
      if (!current || current.deviceId !== target.deviceId || current.sourceId !== target.sourceId)
        return;
      if (!this.usableDescriptor(descriptor)) {
        this.failPlayback();
        return;
      }
      this.descriptorTarget = target;
      this.descriptor = descriptor;
      this.schedulePlaybackRefresh(target);
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
    this.switchPrimary()?.focus();
  }

  private switchPrimary(): HTMLElement | null {
    const dialog = this.querySelector<HTMLElement>('.runtime-task__switch-dialog');
    if (!dialog) return null;
    return (
      dialog.querySelector<HTMLElement>('[role="option"]') ??
      dialog.querySelector<HTMLElement>('[data-action="keep-task"]')
    );
  }

  private syncSwitchTrap(): void {
    const dialog = this.switchOpen
      ? this.querySelector<HTMLElement>('.runtime-task__switch-dialog')
      : null;
    if (!dialog) {
      this.switchTrap?.detach();
      this.switchTrap = null;
      this.switchTrapTarget = null;
      return;
    }
    if (this.switchTrap && this.switchTrapTarget === dialog) return;
    this.switchTrap?.detach();
    this.switchTrapTarget = dialog;
    this.switchTrap = createDialogFocusTrap(dialog, {
      onEscape: () => this.closeSwitchTask(),
      focusables: () => [
        ...dialog.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ),
      ],
      primary: () => this.switchPrimary(),
    });
    this.switchTrap.attach();
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

  private focusStatus(): void {
    void this.updateComplete.then(() => {
      this.querySelector<HTMLElement>('.runtime-task__status')?.focus({ preventScroll: true });
    });
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
      } else if (
        previous === undefined &&
        previousTask !== null &&
        (job.state === 'accepted' || job.state === 'running')
      ) {
        messages.push(`Job ${job.id} ${statusLabel(job.state)}`);
      }
    }
    this.lastJobStates = nextJobs;
    if (messages.length) this.announcement = messages.join('. ');
  }

  private async locateDevice(event: CustomEvent<{ deviceId: string }>): Promise<void> {
    event.stopPropagation();
    this.device = event.detail.deviceId;
    // A located device is an explicit expand action, so persist it even when
    // mobile already force-expanded the panel without touching the preference.
    this.setDevicesExpanded(true);
    this.syncUrl();
    await this.updateComplete;
    this.querySelector<RuntimeDevices>('runtime-devices')?.focusDevice(
      this.device,
      this.reducedMotion,
    );
  }

  private selectHistoryEvent(event: CustomEvent<{ eventId: string }>): void {
    const eventId = event.detail.eventId;
    this.historyEventId = eventId;
    this.historyOccurredAt =
      this.projection?.steps.find((step) => step.eventId === eventId)?.occurredAt ?? '';
    this.history = { ...this.history, event: eventId };
    this.syncUrl();
  }

  private get historyMode(): boolean {
    return !this.historyRejected && isHistoryState(this.history);
  }

  /**
   * A cold history link has no in-memory steps to freeze. Showing the live
   * snapshot under a past-time banner would present present facts as past ones,
   * so strip the cursor, show a one-time notice, and load the live view.
   */
  private recoverColdHistory(): void {
    if (!isHistoryState(this.history) || this.projection) return;
    this.historyEventId = '';
    this.historyOccurredAt = '';
    this.historyRejected = true;
    this.coldHistoryStripped = true;
    this.historyNotice = 'History is no longer available from this link; showing live activity.';
    window.history.replaceState(
      {},
      '',
      toLiveRuntimeTaskUrl(this.taskId, { view: this.view, device: this.device }),
    );
    // Let the router re-render from the live URL without blocking this load.
    queueMicrotask(() => {
      window.dispatchEvent(new HashChangeEvent('hashchange'));
    });
  }

  /**
   * An event selection only counts while its step is actually in the frozen
   * steps; trusting the id alone lets a stale selection freeze an empty list.
   */
  private hasHistorySelection(steps: RuntimeEvent[]): boolean {
    if (!this.historyEventId) return false;
    return steps.some((step) => step.eventId === this.historyEventId);
  }

  private historyTime(): string {
    if (this.history?.at) return this.history.at;
    if (this.history?.event) {
      const fromProjection = this.projection?.steps.find(
        (step) => step.eventId === this.history?.event,
      )?.occurredAt;
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
    window.history.replaceState(
      {},
      '',
      toLiveRuntimeTaskUrl(this.taskId, {
        view: this.view,
        device: this.device,
      }),
    );
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  }

  private cleanupTask(): void {
    this.generation += 1;
    this.closeStream?.();
    this.closeStream = null;
    this.abort?.abort();
    this.abort = null;
    this.timelineAbort?.abort();
    this.timelineAbort = null;
    this.stopPlayback();
    this.switchAbort?.abort();
    this.switchAbort = null;
    this.switchTrap?.detach();
    this.switchTrap = null;
    this.switchTrapTarget = null;
    this.switchOpen = false;
    this.switchTasks = [];
    this.switchLoading = false;
    this.switchError = '';
    this.timelineLoading = false;
    this.timelineError = '';
    this.timelineHasMore = false;
    this.timelineOldest = null;
    this.projection = null;
  }

  private onProjectionChange(event: CustomEvent<TaskProjection>): void {
    event.stopPropagation();
    const snapshot = event.detail;
    this.installSnapshot(snapshot, snapshot.cursor);
  }

  private async load(): Promise<void> {
    this.recoverColdHistory();
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
      if (!this.historyMode) {
        // Seed the timeline before SSE so a page opened on an existing task
        // renders its history immediately, then stream only newer deltas.
        await this.seedTimeline(generation, taskId);
        if (generation !== this.generation || taskId !== this.taskId) return;
        this.openStream(generation, taskId);
      }
    } catch (error) {
      if (generation !== this.generation || this.abort?.signal.aborted) return;
      this.connection = 'offline';
      this.error = error instanceof ApiError ? error.message : 'Network error loading Runtime task';
    }
  }

  private async seedTimeline(generation: number, taskId: string): Promise<void> {
    this.timelineAbort?.abort();
    const controller = new AbortController();
    this.timelineAbort = controller;
    this.timelineLoading = true;
    this.timelineError = '';
    this.timelineHasMore = false;
    this.timelineOldest = null;
    let timedOut = false;
    let timeout: ReturnType<typeof setTimeout> | null = null;
    try {
      // The seed must never gate live updates indefinitely: a stalled events
      // request opens SSE anyway and leaves the timeline in an error/Retry
      // state.
      const response = await Promise.race([
        listRuntimeTaskEvents(
          this.workspaceId,
          taskId,
          timelinePageSize,
          undefined,
          controller.signal,
        ),
        new Promise<never>((_resolve, reject) => {
          timeout = setTimeout(() => {
            timedOut = true;
            controller.abort();
            reject(new Error('Timeline request timed out'));
          }, timelineSeedTimeoutMs);
        }),
      ]);
      if (generation !== this.generation || taskId !== this.taskId || controller.signal.aborted)
        return;
      const page = response.result;
      if (!page) throw new ApiError(500, 'Timeline response missing');
      this.projection?.mergeSteps(page.events);
      this.revision += 1;
      this.timelineHasMore = page.hasMore;
      this.timelineOldest = page.oldestCursor;
    } catch (error) {
      if (generation !== this.generation || taskId !== this.taskId) return;
      if (controller.signal.aborted && !timedOut) return;
      this.timelineError = timedOut
        ? 'Timeline request timed out'
        : error instanceof ApiError
          ? error.message
          : 'Unable to load timeline';
    } finally {
      if (timeout !== null) clearTimeout(timeout);
      if (generation === this.generation && taskId === this.taskId) this.timelineLoading = false;
    }
  }

  private retryTimeline(): void {
    if (this.timelineHasMore && this.timelineOldest !== null) void this.loadEarlier();
    else void this.seedTimeline(this.generation, this.taskId);
  }

  private async loadEarlier(): Promise<void> {
    const before = this.timelineOldest;
    if (this.timelineLoading || before === null || !this.timelineHasMore) return;
    const generation = this.generation;
    const taskId = this.taskId;
    this.timelineAbort?.abort();
    const controller = new AbortController();
    this.timelineAbort = controller;
    this.timelineLoading = true;
    this.timelineError = '';
    try {
      const response = await listRuntimeTaskEvents(
        this.workspaceId,
        taskId,
        timelinePageSize,
        before,
        controller.signal,
      );
      if (generation !== this.generation || taskId !== this.taskId || controller.signal.aborted)
        return;
      const page = response.result;
      if (!page) throw new ApiError(500, 'Timeline response missing');
      this.projection?.mergeSteps(page.events);
      this.revision += 1;
      this.timelineHasMore = page.hasMore;
      this.timelineOldest = page.oldestCursor ?? before;
    } catch (error) {
      if (generation !== this.generation || taskId !== this.taskId || controller.signal.aborted)
        return;
      this.timelineError =
        error instanceof ApiError ? error.message : 'Unable to load earlier events';
    } finally {
      if (generation === this.generation && taskId === this.taskId) this.timelineLoading = false;
    }
  }

  private async loadHistory(steps: RuntimeEvent[]): Promise<void> {
    if (!steps.length && !this.hasHistorySelection(steps)) {
      // Nothing to freeze and no explicit selection to restore: fail closed to
      // the live view instead of freezing a snapshot with an empty timeline.
      this.cleanupTask();
      if (this.taskId) await this.load();
      return;
    }
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
      this.error =
        error instanceof ApiError ? error.message : 'Network error loading Runtime history';
    }
  }

  private async refreshProjection(): Promise<void> {
    // History is a frozen projection; refreshing it would replace past facts
    // with the current live snapshot under the read-only banner.
    if (this.historyMode) return;
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
      this.error =
        error instanceof ApiError ? error.message : 'Network error refreshing Runtime task';
    }
  }

  private installSnapshot(snapshot: TaskProjection, cursor: number): void {
    if (this.projection) this.projection.replaceWithSnapshot(snapshot, cursor);
    else this.projection = new RuntimeProjection(snapshot, { history: this.historyMode });
    this.revision += 1;
    this.invalidatePlaybackTarget();
  }

  /**
   * Descriptors are bound to the device/source that issued them. If a refreshed
   * projection no longer resolves the same target (or none), fail closed.
   */
  private invalidatePlaybackTarget(): void {
    if (!this.descriptor) return;
    const target = this.playbackTarget;
    const current = this.descriptorTarget;
    if (
      !target ||
      !current ||
      target.deviceId !== current.deviceId ||
      target.sourceId !== current.sourceId
    ) {
      this.stopPlayback();
    }
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
          // Live device facts can remove or replace the playing source; the
          // descriptor (and its rendered alt) must not outlive that target.
          this.invalidatePlaybackTarget();
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
      ${
        this.playbackError
          ? html`<p class="runtime-task__error" role="status" data-playback-error>
              ${this.playbackError}
            </p>`
          : nothing
      }
      ${
        this.descriptor
          ? html`<runtime-playback
              .descriptor=${this.descriptor}
              .alt=${this.playbackAlt()}
            ></runtime-playback>`
          : nothing
      }
      <section class="runtime-task__timeline" aria-label="Timeline">
        ${
          this.timelineLoading
            ? html`<p class="runtime-task__timeline-state" data-timeline-loading>
                Loading activity…
              </p>`
            : nothing
        }
        ${
          this.timelineError
            ? html`<div class="runtime-task__timeline-error" data-timeline-error>
                <p role="alert">${this.timelineError}</p>
                <button type="button" data-action="retry-timeline" @click=${this.retryTimeline}>
                  Retry
                </button>
              </div>`
            : nothing
        }
        ${
          this.timelineHasMore
            ? html`<button
                type="button"
                class="runtime-task__timeline-more"
                data-action="load-earlier"
                ?disabled=${this.timelineLoading}
                @click=${this.loadEarlier}
              >
                ${this.timelineLoading ? 'Loading earlier events…' : 'Load earlier events'}
              </button>`
            : nothing
        }
        ${
          !this.timelineLoading &&
          !this.timelineError &&
          !this.timelineHasMore &&
          projection.steps.length === 0
            ? html`<p class="runtime-task__timeline-state" data-timeline-empty>
                No timeline events yet
              </p>`
            : nothing
        }
      </section>
      <section
        class="runtime-workbench"
        data-view=${this.activePanel}
        style=${`--runtime-device-width: ${currentWidth}px`}
      >
        <div class="runtime-workbench__main">
          <div class="runtime-view-tabs-row">
            <div
              class="runtime-view-tabs"
              role="tablist"
              aria-label="Runtime view"
              @keydown=${this.onTabKey}
            >
              <button
                type="button"
                role="tab"
                data-view="loop"
                id="runtime-tab-loop"
                aria-controls="runtime-panel"
                aria-selected=${this.activePanel === 'loop'}
                tabindex=${this.activePanel === 'loop' ? '0' : '-1'}
                @click=${() => this.selectPanel('loop', false)}
              >
                Agent Loop
              </button>
              <button
                type="button"
                role="tab"
                data-view="flow"
                id="runtime-tab-flow"
                aria-controls="runtime-panel"
                aria-selected=${this.activePanel === 'flow'}
                tabindex=${this.activePanel === 'flow' ? '0' : '-1'}
                @click=${() => this.selectPanel('flow', false)}
              >
                Flow
              </button>
              ${
                this.mobile
                  ? html`<button
                      type="button"
                      role="tab"
                      data-view="devices"
                      id="runtime-tab-devices"
                      aria-controls="runtime-devices-panel"
                      aria-selected=${this.activePanel === 'devices'}
                      tabindex=${this.activePanel === 'devices' ? '0' : '-1'}
                      @click=${() => this.selectPanel('devices', false)}
                    >
                      Devices
                    </button>`
                  : nothing
              }
            </div>
            <button type="button" class="runtime-task__devices-anchor" @click=${this.jumpToDevices}>
              Devices
            </button>
          </div>
          <div
            role="tabpanel"
            id="runtime-panel"
            class="runtime-workbench__panel"
            aria-labelledby=${`runtime-tab-${this.view}`}
          >
            ${
              this.view === 'flow'
                ? html`<runtime-flow
                    .projection=${projection}
                    .revision=${this.revision}
                    .reducedMotion=${this.reducedMotion}
                    @locate-device=${this.locateDevice}
                    @event-select=${this.selectHistoryEvent}
                  ></runtime-flow>`
                : html`<runtime-loop
                    .projection=${projection}
                    .revision=${this.revision}
                    .readOnly=${this.historyMode}
                    .loading=${this.timelineLoading}
                    @locate-device=${this.locateDevice}
                    @refresh-request=${this.refreshProjection}
                    @projection-change=${this.onProjectionChange}
                  ></runtime-loop>`
            }
          </div>
        </div>
        <div class="runtime-seam">
          <div
            class="runtime-seam__handle"
            role="separator"
            tabindex=${this.devicesExpanded ? '0' : '-1'}
            aria-label="Resize associated devices. Use Left and Right arrow keys in 24 pixel steps."
            aria-orientation="vertical"
            aria-valuenow=${currentWidth}
            aria-valuemin=${this.devicesExpanded ? minimumDeviceWidth : collapsedDeviceWidth}
            aria-valuemax=${this.maximumDeviceWidth}
            aria-valuetext=${this.devicesExpanded ? `${currentWidth} pixels` : 'Devices collapsed, 44 pixels'}
            @keydown=${this.onSeamKey}
            @pointerdown=${this.onPointerDown}
            @pointermove=${this.onPointerMove}
            @pointerup=${this.onPointerEnd}
            @pointercancel=${this.onPointerEnd}
          ></div>
          <button
            type="button"
            class="runtime-seam__toggle"
            aria-controls="runtime-devices-panel"
            aria-expanded=${this.devicesExpanded}
            aria-label=${this.devicesExpanded ? 'Collapse devices' : 'Expand devices'}
            @click=${this.toggleDevices}
          >
            ${this.devicesExpanded ? '›' : '‹'}
          </button>
        </div>
        <aside
          id="runtime-devices-panel"
          class="runtime-devices-panel ${this.devicesExpanded ? '' : 'is-collapsed'}"
          role=${this.mobile ? 'tabpanel' : 'complementary'}
          aria-labelledby=${this.mobile ? 'runtime-tab-devices' : nothing}
          style="width: ${currentWidth}px;"
        >
          <runtime-devices
            .projection=${projection}
            .revision=${this.revision}
            .selectedDevice=${this.device}
            ?hidden=${!this.devicesExpanded}
            @locate-device=${this.locateDevice}
          ></runtime-devices>
          ${
            this.devicesExpanded
              ? nothing
              : html`<button
                  type="button"
                  class="runtime-devices-rail"
                  aria-controls="runtime-devices-panel"
                  aria-expanded="false"
                  aria-label="Expand devices"
                  @click=${() => this.setDevicesExpanded(true)}
                >
                  <span>Devices</span>
                </button>`
          }
        </aside>
      </section>
    `;
  }

  private renderHistoryBanner() {
    if (!this.historyMode) return nothing;
    const time = this.historyTime();
    return html`<section
      class="runtime-task__history"
      data-history-banner
      aria-labelledby="runtime-history-title"
    >
      <strong id="runtime-history-title">History · read-only</strong>
      ${time ? html`<span> · ${time}</span>` : nothing}
      <button type="button" data-action="back-to-live" @click=${this.backToLive}>
        Back to live
      </button>
    </section>`;
  }

  private renderHistoryNotice() {
    if (!this.historyNotice) return nothing;
    return html`<p class="runtime-task__history-notice" data-history-notice role="status">
      ${this.historyNotice}
    </p>`;
  }

  private renderConnectionBanner() {
    if (this.connection !== 'offline') return nothing;
    return html`<p class="runtime-task__connection" data-connection="offline" role="status">
      <strong>Offline</strong> · showing the last known state. Live updates resume when the
      connection returns.
    </p>`;
  }

  render() {
    const projection = this.projection;
    const pendingApprovals =
      projection?.approvals.filter((approval) => approval.state === 'pending').length ?? 0;
    return html`
      <article class="runtime-task" aria-busy=${projection ? 'false' : 'true'}>
        <nav class="runtime-task__navigation" aria-label="Runtime task navigation">
          <a href="#/runtime">← Runtime</a>
          <button
            type="button"
            class="runtime-task__switch"
            data-action="switch-task"
            @click=${this.openSwitchTask}
          >
            Switch task
          </button>
        </nav>
        <div class="runtime-announcer" data-announcer aria-live="polite" aria-atomic="true">
          ${this.announcement}
        </div>
        <header class="runtime-task-header">
          <h1 class="runtime-objective" tabindex="-1">${this.objectiveLabel}</h1>
          <div class="runtime-task__status" tabindex="-1">
            ${projection?.task.status ?? this.connection}
          </div>
          <p class="runtime-task__intervention ${pendingApprovals ? 'has-approval' : ''}">
            ${pendingApprovals ? `Approval required (${pendingApprovals})` : 'No intervention required'}
          </p>
          ${
            projection
              ? html`<runtime-task-controls
                  .workspaceId=${this.workspaceId}
                  .taskId=${this.taskId}
                  .projection=${projection}
                  .revision=${this.revision}
                  .readOnly=${this.historyMode}
                  @projection-change=${this.onProjectionChange}
                  @focus-status-request=${this.focusStatus}
                  @refresh-request=${this.refreshProjection}
                ></runtime-task-controls>`
              : nothing
          }
          ${
            projection && this.playbackTarget && !this.historyMode
              ? html`<div class="runtime-task__playback-actions">
                  <button
                    type="button"
                    data-action="play-evidence"
                    ?disabled=${this.playbackLoading}
                    @click=${this.playEvidence}
                  >
                    ${this.playbackLoading ? 'Loading evidence…' : 'Play evidence'}
                  </button>
                </div>`
              : nothing
          }
          <slot name="task-actions"></slot>
        </header>
        ${this.renderHistoryNotice()} ${this.renderHistoryBanner()} ${this.renderConnectionBanner()}
        ${this.error ? html`<p role="alert" class="runtime-task__error">${this.error}</p>` : ''}
        <details class="runtime-task__details">
          <summary>Details and IDs</summary>
          <dl>
            <dt>Task ID</dt>
            <dd>${this.taskId}</dd>
            <dt>Run ID</dt>
            <dd>${projection?.task.currentRunId ?? 'None'}</dd>
            <dt>Session key</dt>
            <dd>${projection?.task.sessionKey ?? 'None'}</dd>
          </dl>
        </details>
        ${
          projection
            ? this.renderProjection()
            : html`<div class="runtime-loading" data-loading aria-hidden="true">
                <span></span><span></span><span></span>
              </div>`
        }
        ${
          this.switchOpen
            ? html`<div
                class="runtime-task__switch-overlay"
                role="dialog"
                aria-modal="true"
                aria-labelledby="runtime-switch-title"
              >
                <section class="runtime-task__switch-dialog" tabindex="-1">
                  <h2 id="runtime-switch-title">Switch task</h2>
                  ${this.switchLoading ? html`<p role="status">Loading tasks…</p>` : nothing}
                  ${this.switchError ? html`<p role="alert">${this.switchError}</p>` : nothing}
                  <div role="listbox" aria-label="Runtime tasks">
                    ${this.switchTasks.map(
                      (task) =>
                        html` <button
                          type="button"
                          role="option"
                          aria-label=${task.id}
                          aria-selected=${task.id === this.taskId}
                          @click=${() => this.chooseTask(task.id)}
                        >
                          <strong>${task.objective}</strong>
                          <span>${task.id} · ${statusLabel(task.status)}</span>
                        </button>`,
                    )}
                  </div>
                  ${
                    !this.switchLoading && !this.switchError && this.switchTasks.length === 0
                      ? html`<p>No other Runtime tasks</p>`
                      : nothing
                  }
                  <button type="button" data-action="keep-task" @click=${this.closeSwitchTask}>
                    Keep current task
                  </button>
                </section>
              </div>`
            : nothing
        }
      </article>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'view-runtime-task': RuntimeTaskView;
  }
}
