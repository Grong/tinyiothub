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

  private renderLane(channel: 'control' | 'data', events: RuntimeEvent[]): TemplateResult {
    return html`<section class="flow-lane" data-lane=${channel} aria-label=${channel === 'control' ? 'CONTROL visual lane' : 'DATA visual lane'}>
      <h3>${channel.toUpperCase()}</h3>
      ${channel === 'data' ? this.reducedMotion
        ? html`<p class="flow-static"><span aria-hidden="true">●</span> Static data bus</p>`
        : html`<span class="flow-particle" aria-hidden="true"></span><p class="flow-static flow-static--media"><span aria-hidden="true">●</span> Static data bus</p>`
        : html`<span class="flow-control-icon" aria-hidden="true">→</span>`}
      ${events.length ? html`<ol class="flow-lane__events">${events.map((event) => html`<li>
        <button type="button" data-event-id=${event.eventId} data-cursor=${event.cursor}
          @click=${() => this.selectEvent(event.eventId)}>
          <strong>${event.eventType}</strong><span>${event.deviceId ?? 'Task'}</span><small>Cursor ${event.cursor}</small>
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
      <h2>Flow</h2>
      <p class="runtime-flow__note">CONTROL and DATA are visual lanes, not protocol claims.</p>
      <ol class="runtime-flow__ports" aria-label="Associated device ports">
        ${projection.devices.map((device) => html`<li><button type="button" data-device-port data-device-id=${device.deviceId}
          @click=${() => this.dispatchEvent(new CustomEvent('locate-device', {
            detail: { deviceId: device.deviceId }, bubbles: true, composed: true,
          }))}>${device.deviceId}</button></li>`)}
      </ol>
      <div class="runtime-flow__lanes">${this.renderLane('control', controlEdges)}${this.renderLane('data', dataEdges)}</div>
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-flow': RuntimeFlow;
  }
}
