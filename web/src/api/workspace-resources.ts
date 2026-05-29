/**
 * Workspace Resources API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type { PaginatedResponse } from './client.js';

export interface WorkspaceResource {
  id: string;
  workspaceId: string;
  resourceType: string;
  name: string;
  description: string | null;
  filePath: string;
  tags: string[];
  metadata: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ResourceSearchResult extends WorkspaceResource {
  relevance: number;
}

export interface CreateResourceRequest {
  resourceType: string;
  name: string;
  description?: string;
  tags?: string[];
  metadata?: string;
}

export interface UpdateResourceRequest {
  name?: string;
  description?: string;
  tags?: string[];
  metadata?: string;
}

export interface ResourceListParams {
  resourceType?: string;
  page?: number;
  pageSize?: number;
}

export interface ResourceSearchParams {
  q: string;
  type?: string;
  limit?: number;
}

function getWorkspaceId(): string | null {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem('workspace-id') || sessionStorage.getItem('workspace-id');
}

export const workspaceResourceApi = {
  async listResources(params?: ResourceListParams) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<PaginatedResponse<WorkspaceResource>>(
      `/workspaces/${wsId}/resources`,
      params as Record<string, any>,
    );
  },

  async searchResources(params: ResourceSearchParams) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<ResourceSearchResult[]>(
      `/workspaces/${wsId}/resources/search`,
      params as Record<string, any>,
    );
  },

  async getResource(resourceId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<WorkspaceResource>(`/workspaces/${wsId}/resources/${resourceId}`);
  },

  async createResource(data: CreateResourceRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<WorkspaceResource>(`/workspaces/${wsId}/resources`, data);
  },

  async updateResource(resourceId: string, data: UpdateResourceRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPut<WorkspaceResource>(`/workspaces/${wsId}/resources/${resourceId}`, data);
  },

  async deleteResource(resourceId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiDelete<void>(`/workspaces/${wsId}/resources/${resourceId}`);
  },
};
