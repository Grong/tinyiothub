import Hls from 'hls.js';
import { LitElement, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import type { PlaybackDescriptor } from '../../types/runtime.js';
import './runtime-playback.css';

type PlaybackWindow = Window & { __runtimeActivePlayers?: number };

const runtimeTestMode = import.meta.env.VITE_RUNTIME_TEST === '1';

function metric(name: string, fields: Record<string, string | number> = {}): void {
  console.info(`metric=${name}`, fields);
}

/** Test-only visibility into players that still own media. Never exposed in production builds. */
function adjustActivePlayers(delta: number): void {
  if (!runtimeTestMode || typeof window === 'undefined') return;
  const target = window as PlaybackWindow;
  const current =
    typeof target.__runtimeActivePlayers === 'number' ? target.__runtimeActivePlayers : 0;
  target.__runtimeActivePlayers = Math.max(0, current + delta);
}

@customElement('runtime-playback')
export class RuntimePlayback extends LitElement {
  @property({ attribute: false }) descriptor?: PlaybackDescriptor;
  @property() alt = 'Live camera frame';
  @state() private unavailable = false;

  private hls?: Hls;
  private media?: HTMLVideoElement | HTMLImageElement;
  private loadAbort?: AbortController;
  private generation = 0;
  private ownsPlayer = false;

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

  private acquirePlayer(protocol: string): void {
    if (!this.ownsPlayer) {
      this.ownsPlayer = true;
      adjustActivePlayers(1);
      metric('runtime_playback_session', { protocol });
    }
  }

  private releasePlayer(): void {
    if (!this.ownsPlayer) return;
    this.ownsPlayer = false;
    adjustActivePlayers(-1);
  }

  private showUnavailable(generation: number, reason: string): void {
    if (generation !== this.generation) return;
    this.releasePlayer();
    this.unavailable = true;
    metric('runtime_playback_error', { reason });
  }

  private isLive(descriptor: PlaybackDescriptor): boolean {
    if (descriptor.protocol !== 'hls' && descriptor.protocol !== 'mjpeg') return false;
    const expiresAt = Date.parse(descriptor.expiresAt);
    return Number.isFinite(expiresAt) && expiresAt > Date.now();
  }

  private startPlayer(): void {
    this.destroyPlayer();
    const generation = ++this.generation;
    this.loadAbort = new AbortController();
    const descriptor = this.descriptor;
    if (!descriptor) return;

    if (!this.isLive(descriptor)) {
      this.unavailable = true;
      metric('runtime_playback_error', {
        reason:
          descriptor.protocol === 'hls' || descriptor.protocol === 'mjpeg'
            ? 'descriptor_expired'
            : 'unsupported_protocol',
      });
      return;
    }

    const media = this.renderRoot.querySelector<HTMLVideoElement | HTMLImageElement>(
      descriptor.protocol === 'mjpeg' ? 'img' : 'video',
    );
    if (!media) return;
    this.media = media;
    media.addEventListener('error', () => this.showUnavailable(generation, 'media_error'));

    if (descriptor.protocol === 'mjpeg') {
      this.acquirePlayer('mjpeg');
      (media as HTMLImageElement).src = descriptor.url;
      return;
    }

    const video = media as HTMLVideoElement;
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
      this.acquirePlayer('hls_native');
      video.src = descriptor.url;
    } else if (Hls.isSupported()) {
      const hls = new Hls({ enableWorker: true });
      this.hls = hls;
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (generation !== this.generation || !data.fatal) return;
        this.showUnavailable(generation, 'hls_fatal');
      });
      this.acquirePlayer('hls');
      hls.loadSource(descriptor.url);
      hls.attachMedia(video);
    } else {
      queueMicrotask(() => this.showUnavailable(generation, 'hls_unsupported'));
    }
  }

  private destroyPlayer(): void {
    this.generation += 1;
    this.loadAbort?.abort();
    this.loadAbort = undefined;
    if (this.hls) {
      const hls = this.hls;
      this.hls = undefined;
      try {
        hls.destroy();
      } catch {
        metric('runtime_playback_destroy_failure', { protocol: 'hls' });
      }
    }

    const media = this.media;
    this.media = undefined;
    if (media instanceof HTMLVideoElement) {
      media.pause();
      media.removeAttribute('src');
      media.load();
    } else {
      media?.removeAttribute('src');
    }
    this.releasePlayer();
  }

  render() {
    return html`
      <div class="runtime-playback">
        ${
          this.descriptor?.protocol === 'mjpeg'
            ? html`<img
                class="runtime-playback__media"
                alt=${this.alt}
                .src=${this.descriptor.url}
              />`
            : this.descriptor?.protocol === 'hls'
              ? html`<video
                  class="runtime-playback__media"
                  controls
                  preload="none"
                  playsinline
                  aria-label="Live camera video"
                ></video>`
              : nothing
        }
        ${
          this.unavailable
            ? html`<p class="runtime-playback__unavailable" role="status">
                Playback unavailable <code>playback_unavailable</code>
              </p>`
            : nothing
        }
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
