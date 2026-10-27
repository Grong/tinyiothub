import { ApiClient, ApiError } from './client.js';
import type { PlaybackDescriptor } from '../types/runtime.js';

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
