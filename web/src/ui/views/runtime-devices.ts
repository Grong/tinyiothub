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
    const health = device.health ?? 'unknown';
    const healthTone = health === 'ok' ? 'ok'
      : health === 'attention' ? 'warn'
        : health === 'bad' ? 'danger'
          : 'muted';
    const showHealth = healthTone !== 'ok';
    const hasLocation = location !== 'Not reported';
    const hasSourceFallback = !declarationSource && device.sourceIds.length > 0;
    return html`<article
      class="runtime-device-card ${this.selectedDevice === device.deviceId ? 'is-selected' : ''}"
      data-device-card
      data-device-id=${device.deviceId}
      data-health=${healthTone}
    >
      <div class="runtime-device-card__top">
        <span class="runtime-device-card__health-dot" aria-hidden="true"></span>
        <h3 tabindex="-1"><button class="unstyled" type="button" data-device-id=${device.deviceId}
          @click=${() => this.locate(device.deviceId)}
          @keydown=${this.onDeviceKey}>${device.deviceId}</button></h3>
        <span class="runtime-device-card__availability ${activeJobs.length ? 'is-busy' : ''}"
          >${activeJobs.length ? 'Busy' : 'Ready'}</span
        >
      </div>
      ${activeJobs.map((job) => html`<p class="runtime-device-card__job" data-active-job=${job.id}>Job ${job.id}: ${job.state}</p>`)}
      <p class="runtime-device-card__meta">
        ${showHealth
          ? html`<span class="runtime-device-card__health">${health}</span>`
          : html`<span class="runtime-device-card__sr">Health ${health}</span>`}
        ${hasLocation
          ? html`${showHealth ? html`<span aria-hidden="true">·</span>` : nothing}<span
              >${location}</span
            >`
          : nothing}
        ${hasSourceFallback
          ? html`${showHealth || hasLocation
                ? html`<span aria-hidden="true">·</span>`
                : nothing}<span>${device.sourceIds.join(', ')}</span>`
          : nothing}
      </p>
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
      <header class="runtime-devices__head">
        <h2>Devices <span class="runtime-devices__count">${projection.devices.length}</span></h2>
      </header>
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
