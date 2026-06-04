/**
 * Knowledge Graph API
 */

import { apiGet, apiPost, apiPut, apiDelete, apiUpload } from './client.js';

export interface KnowledgeDocument {
  id: string;
  workspaceId: string;
  resourceType: string;
  name: string;
  description: string | null;
  content: string | null;
  filePath: string;
  fileSize: number | null;
  tags: string[];
  metadata: string | null;
  parseStatus: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface KnowledgeEntity {
  id: string;
  workspaceId: string;
  sourceDocumentId: string;
  entityType: string;
  name: string;
  description: string | null;
  properties: Record<string, unknown>;
  tags: string[];
  fileIds: string[];
  deviceId: string | null;
  confidence: number;
  createdAt: string;
  updatedAt: string;
}

export interface KnowledgeRelation {
  id: string;
  workspaceId: string;
  sourceEntityId: string;
  targetEntityId: string;
  relationType: string;
  properties: Record<string, unknown>;
  confidence: number;
}

export interface KnowledgeParseJob {
  id: string;
  documentId: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  errorMessage: string | null;
  resultSummary: ParseResultSummary | null;
  createdAt: string;
  updatedAt: string;
}

export interface ParseResultSummary {
  entityCount: number;
  relationCount: number;
  diff: ParseDiff | null;
}

export interface ParseDiff {
  added: number;
  removed: number;
  modified: number;
}

export interface CreateKnowledgeDocumentRequest {
  title: string;
  content: string;
  tags?: string[];
  fileIds?: string[];
}

export interface UpdateKnowledgeDocumentRequest {
  title?: string;
  content?: string;
  tags?: string[];
}

export interface PreviewParseRequest {
  content: string;
}

export interface PreviewParseResponse {
  entities: KnowledgeEntity[];
  relations: KnowledgeRelation[];
}

export interface KnowledgeSearchResult {
  entity: KnowledgeEntity;
  relations: KnowledgeRelation[];
  sourceSnippet: string;
  relevance: number;
}

export interface DocumentListResponse {
  data: KnowledgeDocument[];
  pagination: {
    page: number;
    pageSize: number;
    totalPages: number;
    totalCount: number;
  };
}

function getWorkspaceId(): string | null {
  if (typeof window === 'undefined') return null;
  return localStorage.getItem('workspace-id') || sessionStorage.getItem('workspace-id');
}

export const knowledgeApi = {
  // Documents
  async listDocuments(params?: { q?: string; tags?: string; status?: string; page?: number; pageSize?: number }) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<DocumentListResponse>(`/workspaces/${wsId}/knowledge/documents`, params as Record<string, any>);
  },

  async createDocument(data: CreateKnowledgeDocumentRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<KnowledgeDocument>(`/workspaces/${wsId}/knowledge/documents`, data);
  },

  async getDocument(documentId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeDocument>(`/workspaces/${wsId}/knowledge/documents/${documentId}`);
  },

  async updateDocument(documentId: string, data: UpdateKnowledgeDocumentRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPut<KnowledgeDocument>(`/workspaces/${wsId}/knowledge/documents/${documentId}`, data);
  },

  async deleteDocument(documentId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiDelete<void>(`/workspaces/${wsId}/knowledge/documents/${documentId}`);
  },

  // Parse
  async triggerParse(documentId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<{ parseId: string }>(`/workspaces/${wsId}/knowledge/documents/${documentId}/parse`);
  },

  async previewParse(documentId: string, data: PreviewParseRequest) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPost<PreviewParseResponse>(`/workspaces/${wsId}/knowledge/documents/${documentId}/preview`, data);
  },

  async getParseJob(jobId: string) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeParseJob>(`/workspaces/${wsId}/knowledge/parse/${jobId}`);
  },

  // Entities & Relations
  async listEntities(params?: { entityType?: string; tags?: string; documentId?: string }) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeEntity[]>(`/workspaces/${wsId}/knowledge/entities`, params as Record<string, any>);
  },

  async updateEntity(entityId: string, data: Record<string, unknown>) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiPut<KnowledgeEntity>(`/workspaces/${wsId}/knowledge/entities/${entityId}`, data);
  },

  async listRelations() {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeRelation[]>(`/workspaces/${wsId}/knowledge/relations`);
  },

  // Search
  async searchKnowledge(params: { q: string; entityType?: string; tags?: string; limit?: number }) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<KnowledgeSearchResult[]>(`/workspaces/${wsId}/knowledge/search`, params as Record<string, any>);
  },

  // Context (for debugging)
  async getContext() {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    return apiGet<string>(`/workspaces/${wsId}/knowledge/context`);
  },

  // File upload (reuses existing upload endpoint)
  async uploadFile(file: File, onProgress?: (pct: number) => void) {
    const wsId = getWorkspaceId();
    if (!wsId) throw new Error('No workspace selected');
    const formData = new FormData();
    formData.append('file', file);
    return apiUpload<{ filePath: string; fileSize: number }>(
      `/workspaces/${wsId}/resources/upload`,
      formData,
      onProgress,
    );
  },
};
