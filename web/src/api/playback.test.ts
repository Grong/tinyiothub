import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from './client.js';
import { getPlaybackDescriptor } from './playback.js';

describe('getPlaybackDescriptor', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('requests the workspace/device/source descriptor with the caller abort signal', async () => {
    const controller = new AbortController();
    vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify({
      code: 0,
      msg: '',
      result: {
        protocol: 'hls',
        url: 'https://camera.example/live.m3u8?sig=synthetic',
        expires_at: '2099-10-27T10:05:00Z',
      },
    }), { status: 200, headers: { 'Content-Type': 'application/json' } }));

    const descriptor = await getPlaybackDescriptor('ws_1', 'device_1', 'front', controller.signal);

    expect(descriptor).toEqual({
      protocol: 'hls',
      url: 'https://camera.example/live.m3u8?sig=synthetic',
      expiresAt: '2099-10-27T10:05:00Z',
    });
    const [url, init] = vi.mocked(fetch).mock.calls[0];
    expect(String(url)).toContain('/workspaces/ws_1/devices/device_1/sources/front/playback');
    expect(init?.signal).toBe(controller.signal);
  });

  it('rejects a successful envelope with no descriptor', async () => {
    vi.mocked(fetch).mockResolvedValue(new Response(JSON.stringify({
      code: 0,
      msg: '',
      result: null,
    }), { status: 200, headers: { 'Content-Type': 'application/json' } }));

    const request = getPlaybackDescriptor('ws_1', 'device_1', 'front');
    await expect(request).rejects.toBeInstanceOf(ApiError);
    await expect(request).rejects.toMatchObject({ code: 500 });
  });
});
