import { LitElement, html, nothing, type TemplateResult } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import type { EvidenceRef } from '../../types/runtime.js';
import type { RuntimeDeviceRef, RuntimeProjectedJob } from './runtime-projection.js';
import { RuntimeProjection } from './runtime-projection.js';
import './runtime-devices.css';

@customElement('runtime-devices')
export class RuntimeDevices extends LitElement {
  @property({ attribute: false }) projection: RuntimeProjection | null = null;
  @property({ type: Number }) revision = 0;
  @property({ type: String }) selectedDevice: string | null = null;

  createRenderRoot() {
    return this;
  }

  private locate(deviceId: string): void {
    this.dispatchEvent(new CustomEvent('locate-device', {
      detail: { deviceId }, bubbles: true, composed: true,
    }));
  }

  private onDeviceKey(event: KeyboardEvent): void {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    const deviceId = (event.currentTarget as HTMLElement).dataset.deviceId;
    if (!deviceId) return;
    event.preventDefault();
    this.locate(deviceId);
  }

  focusDevice(deviceId: string, reducedMotion: boolean): void {
    const card = [...this.querySelectorAll<HTMLElement>('[data-device-card]')]
      .find((candidate) => candidate.dataset.deviceId === deviceId);
    const heading = card?.querySelector<HTMLElement>('h3');
    if (!card || !heading) return;
    card.scrollIntoView({ behavior: reducedMotion ? 'auto' : 'smooth', block: 'nearest' });
    heading.focus({ preventScroll: true });
  }

  private renderCard(device: RuntimeDeviceRef, jobs: RuntimeProjectedJob[], evidence: EvidenceRef[]): TemplateResult {
    const activeJobs = jobs.filter((job) => job.deviceId === device.deviceId &&
      (job.state === 'accepted' || job.state === 'running'));
    const sources = device.sources ?? [];
    const declarationSource = sources.find((source) => source.dataKind === 'video') ??
      sources.find((source) => source.dataKind === 'image');
    const recentEvidence = declarationSource?.dataKind === 'video'
      ? evidence.filter((item) => item.deviceId === device.deviceId && item.mime.startsWith('image/')).at(-1)
      : undefined;
    const location = [device.zone, device.position].filter((value) => value).join(' / ') || 'Not reported';
    return html`<article class="runtime-device-card ${this.selectedDevice === device.deviceId ? 'is-selected' : ''}"
      data-device-card data-device-id=${device.deviceId}>
      <h3 tabindex="-1"><button type="button" data-device-id=${device.deviceId}
        @click=${() => this.locate(device.deviceId)}
        @keydown=${this.onDeviceKey}>${device.deviceId}</button></h3>
      <p class="runtime-device-card__availability">${activeJobs.length ? 'Busy' : 'Ready'}</p>
      ${activeJobs.map((job) => html`<p class="runtime-device-card__job" data-active-job=${job.id}>Job ${job.id}: ${job.state}</p>`)}
      <dl class="runtime-device-card__facts">
        <dt>Health</dt><dd>${device.health ?? 'unknown'}</dd>
        <dt>Zone/position</dt><dd>${location}</dd>
        <dt>Sources</dt><dd>${device.sourceIds.join(', ') || 'None referenced'}</dd>
      </dl>
      ${declarationSource ? html`<p class="runtime-device-card__declaration" data-declaration-source>
        ${declarationSource.dataKind === 'video' ? 'Video' : 'Image'} ${declarationSource.id}
      </p>` : html`<dl class="runtime-device-card__declaration" data-declaration-fields>
        ${(device.fields ?? []).map((field) => html`<dt>${field.name}</dt>
          <dd>${field.value ?? 'Not reported'}${field.value !== null && field.unit ? html` ${field.unit}` : nothing}</dd>`)}
      </dl>`}
      ${recentEvidence ? html`<p class="runtime-device-card__evidence" data-recent-evidence>
        Recent image evidence ${recentEvidence.evidenceId} · ${recentEvidence.mediaTime}
      </p>` : nothing}
    </article>`;
  }

  render() {
    void this.revision;
    const projection = this.projection;
    if (!projection) return nothing;
    return html`<section class="runtime-devices" aria-label="Associated devices">
      <h2>Associated devices <span>${projection.devices.length}</span></h2>
      ${projection.devices.length ? html`<div class="runtime-devices__list">
        ${projection.devices.map((device) => this.renderCard(device, projection.jobs, projection.evidence))}
      </div>` : html`<p class="runtime-devices__empty">No referenced devices</p>`}
    </section>`;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-devices': RuntimeDevices;
  }
}
