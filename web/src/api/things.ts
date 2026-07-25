/**
 * Thing (物) API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';

export interface Thing {
  id: string;
  name: string;
  thingType: string;
  deviceType: string;
  parentId: string | null;
  templateId: string | null;
  state: string;
  breadcrumb: { id: string; name: string; thingType: string }[];
  ontologySummary: string | null;
  summaryStatus: string;
  createdAt: string;
  updatedAt: string;
}

export interface ThingListResponse {
  items: Thing[];
  total: number;
  limit: number;
  offset: number;
  unassignedResourceCount: number;
}

export interface ThingProperty {
  name: string;
  displayName: string;
  currentValue: unknown;
  value: unknown;
  dataType: string;
  unit: string;
  isReadOnly: boolean;
  minValue?: number;
  maxValue?: number;
  description?: string;
  updatedAt?: string;
}

export interface ThingEvent {
  id: string;
  title: string;
  message: string;
  level: string;
  eventType: string;
  createdAt: string;
  contentPreview: string;
}

export interface KnowledgeDoc {
  id: string;
  name: string;
  resourceType: string;
  filePath: string;
  content: string | null;
  tags: string;
  createdAt: string;
  updatedAt: string;
}

export interface ThingProfileResponse {
  id: string;
  name: string;
  thingType: string;
  deviceType: string;
  parentId: string | null;
  templateId: string | null;
  state: string;
  driverName?: string;
  protocolType?: string;
  ontologySummary: string | null;
  summaryStatus: string;
  breadcrumb: { id: string; name: string; thingType: string }[];
  createdAt: string;
  updatedAt: string;
  properties: ThingProperty[];
  recentEvents: ThingEvent[];
  knowledgeDocs: KnowledgeDoc[];
}

export interface ThingTreeNode {
  id: string;
  name: string;
  thingType: string;
  children: ThingTreeNode[];
}

export interface ThingResource {
  id: string;
  workspaceId: string;
  deviceId: string | null;
  resourceType: string;
  name: string;
  filePath: string;
  content: string | null;
  tags: string;
  createdAt: string;
  updatedAt: string;
}

export interface ConfirmActionResponse {
  thingId: string;
  actionName: string;
  status: string;
  message: string;
  taskId?: string;
}

export const thingApi = {
  async list(params?: Record<string, string>) {
    const qs = params ? `?${new URLSearchParams(params)}` : '';
    return apiGet<ThingListResponse>(`/things${qs}`);
  },

  async get(id: string) {
    return apiGet<Thing>(`/things/${id}`);
  },

  async getProfile(id: string) {
    return apiGet<ThingProfileResponse>(`/things/${id}/profile`);
  },

  async getTree(id: string, depth?: number) {
    const qs = depth !== undefined ? `?depth=${depth}` : '';
    return apiGet<ThingTreeNode[]>(`/things/${id}/tree${qs}`);
  },

  async confirmAction(thingId: string, actionName: string, token: string) {
    return apiPost<ConfirmActionResponse>(`/things/${thingId}/actions/${encodeURIComponent(actionName)}/confirm`, { token });
  },

  async create(data: Record<string, unknown>) {
    return apiPost<Thing>('/things', data);
  },

  async update(id: string, data: Record<string, unknown>) {
    return apiPut<Thing>(`/things/${id}`, data);
  },

  async delete(id: string) {
    return apiDelete<void>(`/things/${id}`);
  },

  async listUnassignedResources() {
    return apiGet<ThingResource[]>('/things/resources/unassigned');
  },

  async attachResource(thingId: string, resourceId: string) {
    return apiPost<void>(`/things/${thingId}/resources`, { resourceId });
  },
};
