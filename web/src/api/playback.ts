import { ApiClient, ApiError } from './client.js';
import type { PlaybackDescriptor } from '../types/runtime.js';

const loopbackHosts = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);

/**
 * Fail-closed URL gate for signed playback descriptors.
 *
 * Only HTTPS or loopback HTTP is acceptable. Credentials in the URL, other
 * schemes (`javascript:`, `data:`, plain HTTP to a remote host) and malformed
 * URLs are rejected before any media element receives the URL.
 */
export function isSafePlaybackUrl(raw: string): boolean {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  if (url.username || url.password) return false;
  if (url.protocol === 'https:') return true;
  if (url.protocol !== 'http:') return false;
  return loopbackHosts.has(url.hostname.toLowerCase());
}

export async function getPlaybackDescriptor(
  workspaceId: string,
  deviceId: string,
  sourceId: string,
  signal?: AbortSignal,
): Promise<PlaybackDescriptor> {
  const response = await ApiClient.get<PlaybackDescriptor>(
    `/workspaces/${workspaceId}/devices/${deviceId}/sources/${sourceId}/playback`,
    undefined,
    signal,
  );
  if (!response.result) throw new ApiError(500, 'Playback descriptor missing');
  return response.result;
}
