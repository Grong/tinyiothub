import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { isSafePlaybackUrl } from '../../api/playback.js';
import type { PlaybackDescriptor } from '../../types/runtime.js';
import './runtime-playback.js';
import type { RuntimePlayback } from './runtime-playback.js';

const { FakeHls } = vi.hoisted(() => {
  type ErrorHandler = (event: string, data: { fatal: boolean }) => void;

  class FakeHls {
    static instances: FakeHls[] = [];
    static Events = { ERROR: 'hlsError' };
    static isSupported = vi.fn(() => true);
    static get activeCount(): number {
      return this.instances.length;
    }

    loadSource = vi.fn();
    attachMedia = vi.fn();
    destroy = vi.fn(() => {
      const index = FakeHls.instances.indexOf(this);
      if (index >= 0) FakeHls.instances.splice(index, 1);
    });
    private errorHandlers: ErrorHandler[] = [];

    constructor(public config: { enableWorker: boolean }) {
      FakeHls.instances.push(this);
    }

    on(event: string, handler: ErrorHandler): void {
      if (event === FakeHls.Events.ERROR) this.errorHandlers.push(handler);
    }

    emitFatalError(): void {
      for (const handler of this.errorHandlers) handler(FakeHls.Events.ERROR, { fatal: true });
    }
  }

  return { FakeHls };
});

vi.mock('hls.js', () => ({ default: FakeHls }));

const mounted: RuntimePlayback[] = [];

function descriptor(
  id: string,
  protocol: PlaybackDescriptor['protocol'] = 'hls',
): PlaybackDescriptor {
  return {
    protocol,
    url:
      protocol === 'hls'
        ? `https://camera.example/${id}.m3u8`
        : `https://camera.example/${id}.mjpg`,
    expiresAt: '2099-10-27T10:05:00Z',
  };
}

async function mount(value?: PlaybackDescriptor): Promise<RuntimePlayback> {
  const element = document.createElement('runtime-playback') as RuntimePlayback;
  mounted.push(element);
  document.body.appendChild(element);
  if (value) element.descriptor = value;
  await element.updateComplete;
  return element;
}

beforeEach(() => {
  FakeHls.instances.length = 0;
  FakeHls.isSupported.mockReturnValue(true);
  vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('');
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => {});
});

afterEach(() => {
  while (mounted.length) mounted.pop()?.remove();
  expect(FakeHls.activeCount).toBe(0);
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe('runtime-playback ownership and cleanup', () => {
  it('destroys Hls when the descriptor changes or the element disconnects', async () => {
    const element = await mount(descriptor('first'));
    const first = FakeHls.instances[0];
    expect(first.loadSource).toHaveBeenCalledWith(descriptor('first').url);
    expect(first.attachMedia).toHaveBeenCalledWith(element.shadowRoot?.querySelector('video'));

    element.descriptor = descriptor('second');
    await element.updateComplete;
    expect(first.destroy).toHaveBeenCalledOnce();
    expect(FakeHls.activeCount).toBe(1);

    element.remove();
    expect(FakeHls.activeCount).toBe(0);
  });

  it('aborts and destroys an Hls instance disconnected during a fragment load', async () => {
    const element = await mount(descriptor('pending'));
    const player = FakeHls.instances[0];
    expect(player.destroy).not.toHaveBeenCalled();

    element.remove();

    expect(player.destroy).toHaveBeenCalledOnce();
    expect(FakeHls.activeCount).toBe(0);
  });

  it('cleans up MJPEG and ignores a late image error from the old descriptor', async () => {
    const element = await mount(descriptor('old-frame', 'mjpeg'));
    const image = element.shadowRoot?.querySelector('img');
    expect(image?.getAttribute('src')).toBe(descriptor('old-frame', 'mjpeg').url);

    element.descriptor = descriptor('live');
    await element.updateComplete;
    image?.dispatchEvent(new Event('error'));
    await element.updateComplete;

    expect(element.shadowRoot?.textContent).not.toContain('Playback unavailable');
    expect(FakeHls.activeCount).toBe(1);
    element.destroy();
    expect(FakeHls.activeCount).toBe(0);
  });

  it('makes destroy idempotent and clears native media without autoplay', async () => {
    const element = await mount(descriptor('native'));
    const video = element.shadowRoot?.querySelector('video') as HTMLVideoElement;
    const pause = vi.spyOn(video, 'pause');
    const load = vi.spyOn(video, 'load');

    element.destroy();
    element.destroy();

    expect(FakeHls.activeCount).toBe(0);
    expect(video.hasAttribute('src')).toBe(false);
    expect(video.autoplay).toBe(false);
    expect(pause).toHaveBeenCalled();
    expect(load).toHaveBeenCalled();
  });

  it('cleans native HLS when switching to MJPEG', async () => {
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('maybe');
    const element = await mount(descriptor('native'));
    const oldVideo = element.shadowRoot?.querySelector('video') as HTMLVideoElement;
    const pause = vi.spyOn(oldVideo, 'pause');
    const load = vi.spyOn(oldVideo, 'load');

    element.descriptor = descriptor('frame', 'mjpeg');
    await element.updateComplete;

    expect(pause).toHaveBeenCalled();
    expect(load).toHaveBeenCalled();
    expect(oldVideo.hasAttribute('src')).toBe(false);
  });

  it('cleans native HLS when the descriptor is cleared', async () => {
    vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockReturnValue('maybe');
    const element = await mount(descriptor('native'));
    const oldVideo = element.shadowRoot?.querySelector('video') as HTMLVideoElement;
    const pause = vi.spyOn(oldVideo, 'pause');
    const load = vi.spyOn(oldVideo, 'load');

    element.descriptor = undefined;
    await element.updateComplete;

    expect(pause).toHaveBeenCalled();
    expect(load).toHaveBeenCalled();
    expect(oldVideo.hasAttribute('src')).toBe(false);
  });

  it('cleans MJPEG when switching to HLS', async () => {
    const element = await mount(descriptor('old-frame', 'mjpeg'));
    const oldImage = element.shadowRoot?.querySelector('img') as HTMLImageElement;

    element.descriptor = descriptor('live');
    await element.updateComplete;

    expect(oldImage.hasAttribute('src')).toBe(false);
    expect(FakeHls.activeCount).toBe(1);
  });

  it('cleans MJPEG when the descriptor is cleared', async () => {
    const element = await mount(descriptor('old-frame', 'mjpeg'));
    const oldImage = element.shadowRoot?.querySelector('img') as HTMLImageElement;

    element.descriptor = undefined;
    await element.updateComplete;

    expect(oldImage.hasAttribute('src')).toBe(false);
  });

  it('does not start an independent descriptor refresh interval', async () => {
    vi.useFakeTimers();
    const element = await mount(descriptor('owned-by-task'));
    const original = element.descriptor;

    await vi.advanceTimersByTimeAsync(10 * 60 * 1000);

    expect(element.descriptor).toBe(original);
    expect(FakeHls.activeCount).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('runtime-playback errors and accessibility', () => {
  it('shows an explicit unavailable state for a fatal Hls error', async () => {
    const element = await mount(descriptor('fatal'));

    FakeHls.instances[0].emitFatalError();
    await element.updateComplete;

    expect(element.shadowRoot?.textContent).toContain('Playback unavailable');
    expect(element.shadowRoot?.textContent).toContain('Captions unavailable');
  });

  it('destroys the Hls instance and media on a fatal error instead of leaving them alive', async () => {
    const element = await mount(descriptor('fatal'));
    const player = FakeHls.instances[0];
    const video = element.shadowRoot?.querySelector('video') as HTMLVideoElement;

    player.emitFatalError();
    await element.updateComplete;

    expect(player.destroy).toHaveBeenCalledOnce();
    expect(FakeHls.activeCount).toBe(0);
    expect(video.hasAttribute('src')).toBe(false);
    expect(element.shadowRoot?.textContent).toContain('Playback unavailable');
  });

  it('rejects stale Hls errors after a descriptor switch', async () => {
    const element = await mount(descriptor('stale'));
    const stale = FakeHls.instances[0];
    element.descriptor = descriptor('current');
    await element.updateComplete;

    stale.emitFatalError();
    await element.updateComplete;

    expect(element.shadowRoot?.textContent).not.toContain('Playback unavailable');
    expect(FakeHls.activeCount).toBe(1);
  });

  it('shows unavailable instead of creating a player when Hls is unsupported', async () => {
    FakeHls.isSupported.mockReturnValue(false);
    const element = await mount(descriptor('unsupported'));

    expect(element.shadowRoot?.textContent).toContain('Playback unavailable');
    expect(FakeHls.activeCount).toBe(0);
  });

  it('renders native controls without autoplay and understandable captions text', async () => {
    const element = await mount(descriptor('controls'));
    const video = element.shadowRoot?.querySelector('video') as HTMLVideoElement;

    expect(video.controls).toBe(true);
    expect(video.autoplay).toBe(false);
    expect(video.preload).toBe('none');
    expect(element.shadowRoot?.textContent).toContain('Captions unavailable');
  });

  it('renders MJPEG with understandable alt text and no recording behavior', async () => {
    const element = await mount(descriptor('frame', 'mjpeg'));
    const image = element.shadowRoot?.querySelector('img');

    expect(image?.alt).toBe('Live camera frame');
    expect(element.shadowRoot?.querySelector('video')).toBeNull();
    expect(element.shadowRoot?.textContent).toContain('Captions unavailable');
  });

  it('never assigns an expired MJPEG descriptor to an image element', async () => {
    const element = await mount({
      protocol: 'mjpeg',
      url: 'https://camera.example/expired.mjpg',
      expiresAt: '2026-10-27T10:05:00Z',
    });

    expect(element.shadowRoot?.querySelector('img')).toBeNull();
    expect(element.shadowRoot?.textContent).toContain('Playback unavailable');
    expect(FakeHls.activeCount).toBe(0);
  });

  it('rejects remote plain-HTTP and credential-bearing URLs before media is created', async () => {
    for (const url of [
      'http://camera.example/frame.mjpg',
      'https://user:secret@camera.example/frame.mjpg',
      'javascript:alert(1)',
      'data:image/jpeg;base64,AAAA',
      'not a url',
    ]) {
      const element = await mount({
        protocol: 'mjpeg',
        url,
        expiresAt: '2099-10-27T10:05:00Z',
      });
      expect(element.shadowRoot?.querySelector('img'), url).toBeNull();
      expect(element.shadowRoot?.textContent).toContain('Playback unavailable');
    }
    expect(FakeHls.activeCount).toBe(0);
  });

  it('binds one media error listener at a time while switching descriptors', async () => {
    const add = vi.spyOn(HTMLImageElement.prototype, 'addEventListener');
    const remove = vi.spyOn(HTMLImageElement.prototype, 'removeEventListener');
    const element = await mount(descriptor('first-frame', 'mjpeg'));
    const first = element.shadowRoot?.querySelector('img') as HTMLImageElement;
    const errorAdds = () => add.mock.calls.filter(([type]) => type === 'error').length;
    expect(errorAdds()).toBe(1);

    element.descriptor = descriptor('second-frame', 'mjpeg');
    await element.updateComplete;

    expect(
      remove.mock.calls.some(
        ([type, handler]) => type === 'error' && add.mock.calls[0][1] === handler,
      ),
    ).toBe(true);
    expect(errorAdds()).toBe(2);
    expect(first.getAttribute('src')).toBe(descriptor('second-frame', 'mjpeg').url);
    expect(element.shadowRoot?.querySelectorAll('img')).toHaveLength(1);
  });

  it('assigns each MJPEG descriptor URL to the media element exactly once', async () => {
    const assignments: string[] = [];
    const descriptorProperty = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src');
    if (!descriptorProperty?.set) throw new Error('missing jsdom src setter');
    const setSrc = descriptorProperty.set;
    const spy = vi.spyOn(HTMLImageElement.prototype, 'src', 'set').mockImplementation(function (
      this: HTMLImageElement,
      value: string,
    ) {
      assignments.push(value);
      setSrc.call(this, value);
    });

    const element = await mount(descriptor('first-frame', 'mjpeg'));
    expect(assignments).toEqual([descriptor('first-frame', 'mjpeg').url]);

    element.descriptor = descriptor('second-frame', 'mjpeg');
    await element.updateComplete;
    expect(assignments).toEqual([
      descriptor('first-frame', 'mjpeg').url,
      descriptor('second-frame', 'mjpeg').url,
    ]);

    spy.mockRestore();
  });
});

describe('isSafePlaybackUrl', () => {
  it('accepts HTTPS and loopback HTTP only', () => {
    expect(isSafePlaybackUrl('https://camera.example/live.m3u8')).toBe(true);
    expect(isSafePlaybackUrl('http://localhost:8080/frame.mjpg')).toBe(true);
    expect(isSafePlaybackUrl('http://127.0.0.1/frame.mjpg')).toBe(true);
    expect(isSafePlaybackUrl('http://[::1]:9000/frame.mjpg')).toBe(true);
  });

  it('rejects other schemes, remote HTTP, credentials and malformed URLs', () => {
    expect(isSafePlaybackUrl('http://camera.example/frame.mjpg')).toBe(false);
    expect(isSafePlaybackUrl('javascript:alert(1)')).toBe(false);
    expect(isSafePlaybackUrl('data:image/jpeg;base64,AAAA')).toBe(false);
    expect(isSafePlaybackUrl('https://user:secret@camera.example/frame.mjpg')).toBe(false);
    expect(isSafePlaybackUrl('not a url')).toBe(false);
  });
});
