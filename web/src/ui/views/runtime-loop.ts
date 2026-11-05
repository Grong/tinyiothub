import { LitElement, html, nothing, type PropertyValues, type TemplateResult } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { ApiError } from '../../api/client.js';
import { stopRuntimeJob } from '../../api/runtime.js';
import type { EvidenceRef, RuntimeEvent, TaskProjection } from '../../types/runtime.js';
import type { RuntimeProjectedApproval, RuntimeProjectedJob } from './runtime-projection.js';
import { RuntimeProjection } from './runtime-projection.js';
import { TERMINAL_TASK_STATUSES } from './runtime-status.js';
import { safeJson } from './runtime-json.js';
import './runtime-approval.js';
import './runtime-loop.css';

const stopStates = new Set(['accepted', 'running']);

/** Tool events are the generic `tool_name` facts; lifecycle facts use dots. */
function isToolEvent(eventType: string): boolean {
  return !eventType.includes('.');
}

function eventTitle(event: RuntimeEvent): string {
  const label = event.eventType.replaceAll('_', ' ').replaceAll('.', ' · ');
  return label.charAt(0).toUpperCase() + label.slice(1);
}

function clockLabel(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

/** Lifecycle events carry only bookkeeping in their payload. The status
 *  header already states the outcome, so a raw disclosure is noise. */
const bookkeepingKeys = new Set(['stateVersion', 'taskStatus', 'outcome']);

function hasMeaningfulPayload(event: RuntimeEvent): boolean {
  const keys = Object.keys(event.data ?? {});
  return keys.length > 0 && keys.some((key) => !bookkeepingKeys.has(key));
}

/** Surface the values a read returned instead of hiding them behind Details. */
function eventFacts(event: RuntimeEvent): [string, string][] {
  if (!isToolEvent(event.eventType)) return [];
  const state = event.data.state;
  if (!state || typeof state !== 'object' || Array.isArray(state)) return [];
  return Object.entries(state as Record<string, unknown>)
    .filter(([, value]) => value !== null && value !== undefined)
    .slice(0, 4)
    .map(([key, value]) => [key, String(value)]);
}

@customElement('runtime-loop')
export class RuntimeLoop extends LitElement {
  @property({ attribute: false }) projection: RuntimeProjection | null = null;
  @property({ type: Number }) revision = 0;
  @property({ type: Boolean }) readOnly = false;
  @property({ type: Boolean }) loading = false;

  @state() private confirmStopJobId: string | null = null;
  @state() private stoppingJobId: string | null = null;
  @state() private stopUnknown = new Set<string>();

  private generation = 0;
  private controller = new AbortController();
  private currentTaskId = '';

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
    if (!changed.has('projection')) return;
    const taskId = this.projection?.task.id ?? '';
    if (taskId === this.currentTaskId) return;
    this.currentTaskId = taskId;
    this.generation += 1;
    this.controller.abort();
    this.controller = new AbortController();
    this.confirmStopJobId = null;
    this.stoppingJobId = null;
    this.stopUnknown = new Set<string>();
  }

  private locate(deviceId: string, evidenceId?: string): void {
    this.dispatchEvent(
      new CustomEvent('locate-device', {
        detail: evidenceId ? { deviceId, evidenceId } : { deviceId },
        bubbles: true,
        composed: true,
      }),
    );
  }

  private requestRefresh(): void {
    this.dispatchEvent(new CustomEvent('refresh-request', { bubbles: true, composed: true }));
  }

  private deviceLink(deviceId: string): TemplateResult {
    return html`<button
      class="unstyled runtime-loop__device"
      type="button"
      data-device-id=${deviceId}
      @click=${() => this.locate(deviceId)}
    >
      ${deviceId}
    </button>`;
  }

  private stepKind(event: RuntimeEvent): string {
    if (event.evidenceId || /result|done|completed|failed/.test(event.eventType)) return 'Result';
    if (isToolEvent(event.eventType) || event.jobId || event.approvalId) return 'Tool';
    if (event.eventType.startsWith('reason.') || event.eventType === 'run.started') return 'Reason';
    return 'Act';
  }

  private renderEvent(event: RuntimeEvent, approval?: TemplateResult): TemplateResult {
    const kind = this.stepKind(event);
    const facts = eventFacts(event);
    // Facts answer the common question; raw payload stays available only when
    // it carries something the row does not already show.
    const hasPayload = facts.length === 0 && hasMeaningfulPayload(event);
    const isResult = kind === 'Result';
    const failed =
      isResult && (event.eventType === 'task.failed' || this.projection?.task.status === 'failed');
    return html`<li
      class="runtime-event ${isResult ? 'is-terminal' : ''} ${failed ? 'is-failed' : ''}"
      data-event-id=${event.eventId}
      data-kind=${kind}
    >
      <time class="runtime-event__time" datetime=${event.occurredAt}>
        ${clockLabel(event.occurredAt)}
      </time>
      <span class="runtime-event__node" aria-hidden="true"></span>
      <div class="runtime-event__body">
        <header class="runtime-event__header">
          <span class="runtime-event__kind">${kind}</span>
          <strong class="runtime-event__title">${eventTitle(event)}</strong>
          <code class="runtime-event__type">${event.eventType}</code>
        </header>
        ${event.deviceId ? html`<div class="runtime-event__device">${this.deviceLink(event.deviceId)}</div>` : nothing}
        ${
          facts.length
            ? html`<p class="runtime-event__facts">
                ${facts.map(
                  ([label, value]) =>
                    html`<span class="runtime-event__fact"
                      ><span class="runtime-event__fact-label">${label}</span
                      ><span class="runtime-event__fact-value">${value}</span></span
                    >`,
                )}
              </p>`
            : nothing
        }
        ${
          hasPayload
            ? html`<details class="runtime-event__payload">
                <summary>Payload</summary>
                <pre>${safeJson(event.data)}</pre>
              </details>`
            : nothing
        }
        ${approval ?? nothing}
      </div>
    </li>`;
  }

  private renderApproval(approval: RuntimeProjectedApproval): TemplateResult {
    return html`<runtime-approval
      .workspaceId=${approval.workspaceId ?? this.projection?.task.workspaceId ?? ''}
      .taskId=${this.projection?.task.id ?? ''}
      .approval=${approval}
      .readOnly=${this.readOnly}
    ></runtime-approval>`;
  }

  private ownsJob(job: RuntimeProjectedJob): boolean {
    return Boolean(this.projection && job.taskId === this.projection.task.id);
  }

  private canOfferStop(job: RuntimeProjectedJob): boolean {
    const status = this.projection?.task.status;
    return (
      !this.readOnly &&
      status !== undefined &&
      !TERMINAL_TASK_STATUSES.has(status) &&
      this.ownsJob(job) &&
      stopStates.has(job.state) &&
      !this.stopUnknown.has(job.id)
    );
  }

  private openStopConfirmation(job: RuntimeProjectedJob): void {
    if (!this.canOfferStop(job) || this.stoppingJobId) return;
    this.confirmStopJobId = job.id;
  }

  private keepJob(): void {
    if (this.stoppingJobId) return;
    this.confirmStopJobId = null;
  }

  private async requestStop(job: RuntimeProjectedJob): Promise<void> {
    if (!this.canOfferStop(job) || this.stoppingJobId) return;
    const generation = this.generation;
    const taskId = this.projection?.task.id ?? '';
    this.confirmStopJobId = null;
    this.stoppingJobId = job.id;
    this.errorFor(job.id, false);
    try {
      const response = await stopRuntimeJob(
        job.workspaceId ?? this.projection?.task.workspaceId ?? '',
        taskId,
        job.id,
        this.controller.signal,
      );
      if (!response.result) throw new ApiError(500, 'Task projection missing');
      if (generation !== this.generation) return;
      this.dispatchEvent(
        new CustomEvent<TaskProjection>('projection-change', {
          detail: response.result,
          bubbles: true,
          composed: true,
        }),
      );
    } catch {
      if (generation !== this.generation || this.controller.signal.aborted) return;
      this.errorFor(job.id, true);
    } finally {
      if (generation === this.generation) this.stoppingJobId = null;
    }
  }

  private errorFor(jobId: string, unknown: boolean): void {
    this.stopUnknown = new Set(
      unknown ? [...this.stopUnknown, jobId] : [...this.stopUnknown].filter((id) => id !== jobId),
    );
  }

  private renderJob(job: RuntimeProjectedJob): TemplateResult {
    const unknown = job.state === 'cancelled_unknown' || this.stopUnknown.has(job.id);
    const confirming = this.confirmStopJobId === job.id;
    const stopping = this.stoppingJobId === job.id;
    return html`<section class="tool-card" data-job-id=${job.id}>
      <header><strong>${job.tool ?? 'Device job'}</strong><span>${job.state}</span></header>
      ${job.deviceId ? html`<p>${this.deviceLink(job.deviceId)}</p>` : nothing}
      ${
        unknown
          ? html`<p role="alert">Stop result unknown</p>
              ${
                this.readOnly
                  ? nothing
                  : html`<button
                      class="btn"
                      type="button"
                      data-action="refresh-status"
                      @click=${this.requestRefresh}
                    >
                      Refresh status
                    </button>`
              }`
          : nothing
      }
      ${
        !unknown && (confirming || stopping)
          ? html`<div
              class="runtime-loop__stop-confirm"
              role="group"
              aria-label="Stop job confirmation"
            >
              <p>Stops only this job; the rest of the task continues.</p>
              ${
                stopping
                  ? html`<p role="status">Stopping…</p>`
                  : html`<button
                      class="btn"
                      type="button"
                      data-action="keep-job"
                      @click=${this.keepJob}
                    >
                      Keep job
                    </button>`
              }
              <button
                class="btn"
                type="button"
                data-action="confirm-stop"
                data-executable
                ?disabled=${stopping}
                @click=${() => this.requestStop(job)}
              >
                Confirm stop
              </button>
            </div>`
          : nothing
      }
      ${
        !unknown && !confirming && !stopping && this.canOfferStop(job)
          ? html`<button
              class="btn"
              type="button"
              data-action="stop-job"
              data-executable
              @click=${() => this.openStopConfirmation(job)}
            >
              Stop job
            </button>`
          : nothing
      }
      <details>
        <summary>${job.state === 'done' ? 'Done/Details' : 'Details'}</summary>
        <dl>
          <dt>Arguments</dt>
          <dd><pre>${safeJson(job.argsJson)}</pre></dd>
          <dt>Result</dt>
          <dd><pre>${safeJson(job.resultJson)}</pre></dd>
        </dl>
      </details>
    </section>`;
  }

  private renderEvidence(evidence: EvidenceRef): TemplateResult {
    const expiresAt = evidence.expiresAt ? Date.parse(evidence.expiresAt) : Number.NaN;
    if (Number.isFinite(expiresAt) && expiresAt <= Date.now()) {
      return html`<div
        class="runtime-loop__evidence runtime-loop__evidence--expired"
        data-evidence-expired=${evidence.evidenceId}
        role="status"
      >
        <span>Image evidence</span><span>${evidence.evidenceId}</span>
        <small>Image evidence unavailable (expired) · ${evidence.mediaTime}</small>
      </div>`;
    }
    return html`<button
      class="unstyled runtime-loop__evidence"
      type="button"
      data-evidence-id=${evidence.evidenceId}
      @click=${() => this.locate(evidence.deviceId, evidence.evidenceId)}
    >
      <span>Image evidence</span><span>${evidence.evidenceId}</span
      ><small>${evidence.mediaTime}</small>
    </button>`;
  }

  render() {
    void this.revision;
    const projection = this.projection;
    if (!projection) return nothing;

    const approvalById = new Map<string, RuntimeProjectedApproval>();
    for (const approval of projection.approvals) {
      if (typeof approval.id === 'string' && typeof approval.tool === 'string') {
        approvalById.set(approval.id, approval);
      }
    }
    const renderedApprovals = new Set<string>();
    const steps = projection.steps.map((event) => {
      const approval = event.approvalId ? approvalById.get(event.approvalId) : undefined;
      // The request and its later decision share an approval id; render the
      // card once, attached to the first referencing step.
      if (approval && !renderedApprovals.has(approval.id)) {
        renderedApprovals.add(approval.id);
        return this.renderEvent(event, this.renderApproval(approval));
      }
      if (approval) renderedApprovals.add(approval.id);
      return this.renderEvent(event);
    });
    const orphanApprovals = [...approvalById.values()].filter(
      (approval) => !renderedApprovals.has(approval.id),
    );

    const imageEvidence = projection.evidence.filter(
      (evidence) => evidence.deviceId && evidence.mime.startsWith('image/'),
    );
    const hasActivity =
      this.loading ||
      projection.jobs.length ||
      projection.steps.length ||
      projection.evidence.length ||
      projection.approvals.length;
    return html`<section class="runtime-loop" aria-label="Agent Loop">
      <header class="runtime-loop__head">
        <h2 class="runtime-loop__title">Execution</h2>
        <span class="runtime-loop__count">${projection.steps.length} events</span>
      </header>
      ${hasActivity ? nothing : html`<p class="runtime-loop__empty">No agent activity yet</p>`}
      ${
        projection.jobs.length
          ? html`<section class="runtime-loop__jobs" aria-label="Device jobs">
              <h3 class="runtime-loop__subhead">
                Device jobs <span>${projection.jobs.length}</span>
              </h3>
              ${projection.jobs.map((job) => this.renderJob(job))}
            </section>`
          : nothing
      }
      ${
        orphanApprovals.length
          ? html`<div class="runtime-loop__approvals">
              ${orphanApprovals.map((approval) => this.renderApproval(approval))}
            </div>`
          : nothing
      }
      ${
        steps.length
          ? html`<ol class="runtime-loop__steps" aria-label="Agent activity">
              ${steps}
            </ol>`
          : nothing
      }
      ${
        imageEvidence.length
          ? html`<section aria-label="Image evidence" class="runtime-loop__evidence-list">
              <h3 class="runtime-loop__subhead">Image evidence</h3>
              ${imageEvidence.map((evidence) => this.renderEvidence(evidence))}
            </section>`
          : nothing
      }
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-loop': RuntimeLoop;
  }
}
