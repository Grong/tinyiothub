import { LitElement, html, nothing, type TemplateResult } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import type { RuntimeEvent } from '../../types/runtime.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-flow.css';

@customElement('runtime-flow')
export class RuntimeFlow extends LitElement {
  @property({ attribute: false }) projection: RuntimeProjection | null = null;
  @property({ type: Number }) revision = 0;
  @property({ type: Boolean }) reducedMotion = false;

  createRenderRoot() {
    return this;
  }

  private isData(event: RuntimeEvent): boolean {
    return Boolean(event.sourceId || event.evidenceId || event.eventType.startsWith('source.') ||
      event.eventType.startsWith('evidence.'));
  }

  private selectEvent(eventId: string): void {
    this.dispatchEvent(new CustomEvent('event-select', {
      detail: { eventId }, bubbles: true, composed: true,
    }));
  }

  private clockLabel(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return '';
    return date.toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });
  }

  private renderLane(channel: 'control' | 'data', events: RuntimeEvent[]): TemplateResult {
    const empty = events.length === 0;
    return html`<section
      class="flow-lane"
      data-lane=${channel}
      data-empty=${empty ? 'true' : nothing}
      aria-label=${channel === 'control' ? 'CONTROL visual lane' : 'DATA visual lane'}
    >
      <header class="flow-lane__head">
        <h3>${channel.toUpperCase()}</h3>
        <span class="flow-lane__count">${events.length}</span>
        <span class="flow-lane__direction" aria-hidden="true">→</span>
      </header>
      ${channel === 'data' && !empty
        ? this.reducedMotion
          ? html`<p class="flow-static"><span aria-hidden="true">●</span> Static data bus</p>`
          : html`<span class="flow-particle runtime-data-particles" aria-hidden="true"></span><p class="flow-static flow-static--media"><span aria-hidden="true">●</span> Static data bus</p>`
        : nothing}
      ${events.length ? html`<ol class="flow-lane__events">${events.map((event) => html`<li>
        <button class="unstyled" type="button" data-event-id=${event.eventId} data-cursor=${event.cursor}
          aria-label=${`Historical event ${event.eventId}`}
          @click=${() => this.selectEvent(event.eventId)}>
          <strong>${event.eventType}</strong>
          <span class="flow-lane__event-device">${event.deviceId ?? 'Task'}</span>
          <small>${this.clockLabel(event.occurredAt)}</small>
        </button>
      </li>`)}</ol>` : html`<p class="flow-lane__empty">No ${channel} events</p>`}
    </section>`;
  }

  render() {
    void this.revision;
    const projection = this.projection;
    if (!projection) return nothing;
    const controlEdges = projection.steps.filter((event) => !this.isData(event));
    const dataEdges = projection.steps.filter((event) => this.isData(event));
    return html`<section class="runtime-flow" aria-label="Runtime Flow">
      <header class="runtime-flow__head">
        <h2>Flow</h2>
        <span class="runtime-flow__count">${controlEdges.length + dataEdges.length} edges</span>
      </header>
      <p class="runtime-flow__note">
        CONTROL and DATA are visual lanes, not protocol claims.
      </p>
      <section class="runtime-flow__ports-block" aria-label="Associated device ports">
        <h3 class="runtime-flow__label">Devices in this task</h3>
        <ol class="runtime-flow__ports">
          ${projection.devices.map((device) => html`<li><button class="unstyled" type="button" data-device-port data-device-id=${device.deviceId}
            @click=${() => this.dispatchEvent(new CustomEvent('locate-device', {
              detail: { deviceId: device.deviceId }, bubbles: true, composed: true,
            }))}>${device.deviceId}</button></li>`)}
        </ol>
      </section>
      <div class="runtime-flow__lanes">${this.renderLane('control', controlEdges)}${this.renderLane('data', dataEdges)}</div>
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-flow': RuntimeFlow;
  }
}
