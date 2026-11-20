/**
 * 媒体证据读取（AI 抓拍画面）。
 *
 * 证据端点要求工作区鉴权，因此不能用裸 `<img src>`；这里用 Authorization
 * 头取 Blob，调用方负责 `URL.createObjectURL` / `revokeObjectURL` 的生命周期。
 */

import { buildUrl, getAuthToken, getWorkspaceId } from './client.js';

export async function fetchEvidenceBlob(
  evidenceId: string,
  workspaceId?: string,
  signal?: AbortSignal,
): Promise<Blob> {
  const ws = workspaceId || getWorkspaceId();
  if (!ws) throw new Error('缺少工作区上下文');
  const token = getAuthToken();
  const headers: Record<string, string> = {};
  if (token) headers.Authorization = `Bearer ${token}`;
  const response = await fetch(
    buildUrl(`/workspaces/${encodeURIComponent(ws)}/evidence/${encodeURIComponent(evidenceId)}`),
    { headers, credentials: 'include', signal },
  );
  if (!response.ok) {
    throw new Error(response.status === 410 ? '画面已过期' : '画面读取失败');
  }
  return response.blob();
}
