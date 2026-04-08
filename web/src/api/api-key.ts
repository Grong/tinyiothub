/**
 * API Key management
 * 路径: /api-keys (workspace 从 header X-Workspace-Id 获取)
 */

import { apiGet, apiPost, apiDelete } from "./client.js";

export interface ApiKey {
  id: string;
  workspaceId: string;
  name: string;
  prefix: string;
  permissions: string;
  rateLimit: number;
  isEnabled: boolean;
  isRevoked: boolean;
  lastUsedAt: string | null;
  lastUsedIp: string | null;
  requestCount: number;
  expiresAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CreateApiKeyResponse {
  apiKey: ApiKey;
  rawKey: string;
}

export const apiKeyApi = {
  /** List all API keys for current workspace (from X-Workspace-Id header) */
  async list(_workspaceId: string): Promise<ApiKey[]> {
    const res = await apiGet<ApiKey[]>(`/api-keys`);
    return res.result ?? [];
  },

  /** Create a new API key — rawKey is returned only once */
  async create(_workspaceId: string, name: string): Promise<CreateApiKeyResponse> {
    const res = await apiPost<CreateApiKeyResponse>("/api-keys", {
      name,
    });
    if (!res.result) {
      throw new Error("创建失败");
    }
    return res.result;
  },

  /** Revoke (delete) an API key */
  async revoke(id: string): Promise<void> {
    await apiDelete(`/api-keys/${id}`);
  },
};
