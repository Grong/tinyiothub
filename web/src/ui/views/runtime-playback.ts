import Hls from 'hls.js';
import { LitElement, css, html, nothing, type PropertyValues } from 'lit';
import { customElement, property, state } from 'lit/decorators.js';
import { isSafePlaybackUrl } from '../../api/playback.js';
import type { PlaybackDescriptor } from '../../types/runtime.js';

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
  private generation = 0;
  private ownsPlayer = false;
  private errorMedia?: HTMLVideoElement | HTMLImageElement;
  private errorHandler?: () => void;

  static styles = css`
    :host {
      display: block;
    }

    .runtime-playback {
      display: grid;
      gap: var(--space-2);
      min-width: 0;
    }

    .runtime-playback__media {
      display: block;
      width: 100%;
      min-width: 0;
      max-width: 100%;
      max-height: 70vh;
      border-radius: var(--radius-md);
      background: var(--glass-bg);
      object-fit: contain;
    }

    .runtime-playback__unavailable,
    .runtime-playback__captions {
      margin: 0;
      color: var(--runtime-muted, var(--muted));
      font-size: 12px;
    }

    .runtime-playback__unavailable {
      color: var(--runtime-warn, var(--warn));
    }
  `;

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

  /** Single place that decides whether a descriptor may reach a media element. */
  private usableDescriptor(): PlaybackDescriptor | null {
    const descriptor = this.descriptor;
    if (!descriptor) return null;
    if (descriptor.protocol !== 'hls' && descriptor.protocol !== 'mjpeg') return null;
    if (!isSafePlaybackUrl(descriptor.url)) return null;
    const expiresAt = Date.parse(descriptor.expiresAt);
    return Number.isFinite(expiresAt) && expiresAt > Date.now() ? descriptor : null;
  }

  private unavailableReason(descriptor: PlaybackDescriptor): string {
    if (descriptor.protocol !== 'hls' && descriptor.protocol !== 'mjpeg') {
      return 'unsupported_protocol';
    }
    if (!isSafePlaybackUrl(descriptor.url)) return 'unsafe_url';
    return 'descriptor_expired';
  }

  private bindMediaError(media: HTMLVideoElement | HTMLImageElement, generation: number): void {
    this.unbindMediaError();
    const handler = () => this.showUnavailable(generation, 'media_error');
    media.addEventListener('error', handler);
    this.errorMedia = media;
    this.errorHandler = handler;
  }

  private unbindMediaError(): void {
    if (this.errorMedia && this.errorHandler) {
      this.errorMedia.removeEventListener('error', this.errorHandler);
    }
    this.errorMedia = undefined;
    this.errorHandler = undefined;
  }

  private startPlayer(): void {
    this.destroyPlayer();
    const generation = ++this.generation;
    const descriptor = this.descriptor;
    if (!descriptor) return;

    const usable = this.usableDescriptor();
    if (!usable) {
      this.unavailable = true;
      metric('runtime_playback_error', { reason: this.unavailableReason(descriptor) });
      return;
    }

    const media = this.renderRoot.querySelector<HTMLVideoElement | HTMLImageElement>(
      usable.protocol === 'mjpeg' ? 'img' : 'video',
    );
    if (!media) return;
    this.media = media;
    this.bindMediaError(media, generation);

    if (usable.protocol === 'mjpeg') {
      this.acquirePlayer('mjpeg');
      (media as HTMLImageElement).src = usable.url;
      return;
    }

    const video = media as HTMLVideoElement;
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
      this.acquirePlayer('hls_native');
      video.src = usable.url;
    } else if (Hls.isSupported()) {
      const hls = new Hls({ enableWorker: true });
      this.hls = hls;
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (generation !== this.generation || !data.fatal) return;
        this.showUnavailable(generation, 'hls_fatal');
        // A fatal error leaves the instance dead; release the player instead of
        // letting a broken Hls/<video> pair linger until the next descriptor.
        this.destroyPlayer();
      });
      this.acquirePlayer('hls');
      hls.loadSource(usable.url);
      hls.attachMedia(video);
    } else {
      queueMicrotask(() => this.showUnavailable(generation, 'hls_unsupported'));
    }
  }

  private destroyPlayer(): void {
    this.generation += 1;
    this.unbindMediaError();
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
    const usable = this.usableDescriptor();
    return html`
      <div class="runtime-playback">
        ${
          usable?.protocol === 'mjpeg'
            ? html`<img class="runtime-playback__media" alt=${this.alt} />`
            : usable?.protocol === 'hls'
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
