import Hls from 'hls.js';
import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import type { PlaybackDescriptor } from '../../types/runtime.js';
import './runtime-playback.css';

@customElement('runtime-playback')
export class RuntimePlayback extends LitElement {
  @property({ attribute: false }) descriptor?: PlaybackDescriptor;
  @state() private unavailable = false;

  private hls?: Hls;
  private media?: HTMLVideoElement | HTMLImageElement;
  private loadAbort?: AbortController;
  private generation = 0;

  disconnectedCallback(): void {
    this.destroyPlayer();
    super.disconnectedCallback();
  }

  willUpdate(changed: PropertyValues<this>): void {
    if (changed.has('descriptor')) this.unavailable = false;
  }

  updated(changed: PropertyValues<this>): void {
    if (changed.has('descriptor')) this.startPlayer();
  }

  destroy(): void {
    this.destroyPlayer();
  }

  private showUnavailable(generation: number): void {
    if (generation === this.generation) this.unavailable = true;
  }

  private startPlayer(): void {
    this.destroyPlayer();
    const generation = ++this.generation;
    this.loadAbort = new AbortController();
    const descriptor = this.descriptor;
    if (!descriptor) return;

    const media = this.renderRoot.querySelector<HTMLVideoElement | HTMLImageElement>(
      descriptor.protocol === 'mjpeg' ? 'img' : 'video',
    );
    if (!media) return;
    this.media = media;
    media.addEventListener('error', () => this.showUnavailable(generation));

    if (descriptor.protocol === 'mjpeg') {
      (media as HTMLImageElement).src = descriptor.url;
      return;
    }

    const video = media as HTMLVideoElement;
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
      video.src = descriptor.url;
    } else if (Hls.isSupported()) {
      const hls = new Hls({ enableWorker: true });
      this.hls = hls;
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (generation === this.generation && data.fatal) this.showUnavailable(generation);
      });
      hls.loadSource(descriptor.url);
      hls.attachMedia(video);
    } else {
      queueMicrotask(() => this.showUnavailable(generation));
    }
  }

  private destroyPlayer(): void {
    this.generation += 1;
    this.loadAbort?.abort();
    this.loadAbort = undefined;
    this.hls?.destroy();
    this.hls = undefined;

    const media = this.media;
    this.media = undefined;
    if (media instanceof HTMLVideoElement) {
      media.pause();
      media.removeAttribute('src');
      media.load();
    } else {
      media?.removeAttribute('src');
    }
  }

  render() {
    return html`
      <div class="runtime-playback">
        ${this.descriptor?.protocol === 'mjpeg'
          ? html`<img class="runtime-playback__media" alt="Live camera frame" .src=${this.descriptor.url} />`
          : this.descriptor?.protocol === 'hls'
            ? html`<video class="runtime-playback__media" controls preload="none" playsinline aria-label="Live camera video"></video>`
            : nothing}
        ${this.unavailable
          ? html`<p class="runtime-playback__unavailable" role="status">Playback unavailable</p>`
          : nothing}
        <p class="runtime-playback__captions">Captions unavailable</p>
      </div>
    `;
  }
}

declare global {
  interface HTMLElementTagNameMap {
    'runtime-playback': RuntimePlayback;
  }
}
