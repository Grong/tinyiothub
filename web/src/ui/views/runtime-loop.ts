import { LitElement, html, nothing, type TemplateResult } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import type { EvidenceRef, RuntimeEvent } from '../../types/runtime.js';
import type { RuntimeProjectedJob } from './runtime-projection.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-loop.css';

const toolEvents = new Set(['read_device', 'call_device', 'set_device', 'stop_device', 'watch_device']);

function safeJson(value: unknown): string {
  if (value === undefined || value === null) return 'None';
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return 'Unavailable';
  }
}

@customElement('runtime-loop')
export class RuntimeLoop extends LitElement {
  @property({ attribute: false }) projection: RuntimeProjection | null = null;
  @property({ type: Number }) revision = 0;

  createRenderRoot() {
    return this;
  }

  private locate(deviceId: string, evidenceId?: string): void {
    this.dispatchEvent(new CustomEvent('locate-device', {
      detail: evidenceId ? { deviceId, evidenceId } : { deviceId },
      bubbles: true,
      composed: true,
    }));
  }

  private deviceLink(deviceId: string): TemplateResult {
    return html`<button class="runtime-loop__device" type="button" data-device-id=${deviceId}
      @click=${() => this.locate(deviceId)}>${deviceId}</button>`;
  }

  private stepKind(event: RuntimeEvent): string {
    if (event.evidenceId || /result|done|completed|failed/.test(event.eventType)) return 'Result';
    if (toolEvents.has(event.eventType) || event.jobId || event.approvalId) return 'Tool';
    if (event.eventType.startsWith('reason.') || event.eventType === 'run.started') return 'Reason';
    return 'Act';
  }

  private renderEvent(event: RuntimeEvent): TemplateResult {
    return html`<article class="runtime-loop__step" data-event-id=${event.eventId}>
      <header><span>${this.stepKind(event)}</span><strong>${event.eventType}</strong></header>
      ${event.deviceId ? this.deviceLink(event.deviceId) : nothing}
      <details><summary>Details</summary><pre>${safeJson(event.data)}</pre></details>
    </article>`;
  }

  private renderJob(job: RuntimeProjectedJob): TemplateResult {
    return html`<section class="tool-card" data-job-id=${job.id}>
      <header><strong>${job.tool ?? 'Device job'}</strong><span>${job.state}</span></header>
      ${job.deviceId ? html`<p>${this.deviceLink(job.deviceId)}</p>` : nothing}
      ${job.state === 'cancelled_unknown'
        ? html`<p role="alert">Stop result unknown</p>
          <button class="btn" type="button" @click=${() => this.dispatchEvent(new CustomEvent('refresh-request', {
            bubbles: true, composed: true,
          }))}>Refresh status</button>`
        : nothing}
      <details><summary>${job.state === 'done' ? 'Done/Details' : 'Details'}</summary>
        <dl><dt>Arguments</dt><dd><pre>${safeJson(job.argsJson)}</pre></dd>
          <dt>Result</dt><dd><pre>${safeJson(job.resultJson)}</pre></dd></dl>
      </details>
    </section>`;
  }

  private renderEvidence(event: RuntimeEvent, evidence: EvidenceRef): TemplateResult {
    return html`<button class="runtime-loop__evidence" type="button" data-evidence-id=${evidence.evidenceId}
      @click=${() => this.locate(event.deviceId!, evidence.evidenceId)}>
      <span>Image evidence</span><span>${evidence.evidenceId}</span><small>${evidence.mediaTime}</small>
    </button>`;
  }

  render() {
    void this.revision;
    const projection = this.projection;
    if (!projection) return nothing;
    const imageEvents = projection.steps.filter((event) => event.deviceId && event.evidenceId &&
      projection.evidence.some((evidence) => evidence.evidenceId === event.evidenceId && evidence.mime.startsWith('image/')));
    return html`<section class="runtime-loop" aria-label="Agent Loop">
      <h2>Agent Loop</h2>
      ${projection.jobs.length || projection.steps.length ? nothing : html`<p class="runtime-loop__empty">No agent activity yet</p>`}
      <div class="runtime-loop__jobs">${projection.jobs.map((job) => this.renderJob(job))}</div>
      <div class="runtime-loop__steps">${projection.steps.map((event) => this.renderEvent(event))}</div>
      ${imageEvents.length ? html`<section aria-label="Image evidence" class="runtime-loop__evidence-list">
        ${imageEvents.map((event) => this.renderEvidence(event, projection.evidence.find(
          (evidence) => evidence.evidenceId === event.evidenceId,
        )!))}
      </section>` : nothing}
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-loop': RuntimeLoop;
  }
}
