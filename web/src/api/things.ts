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

export const thingApi = {
  async list(params?: Record<string, string>) {
    const qs = params ? `?${new URLSearchParams(params)}` : '';
    return apiGet<ThingListResponse>(`/things${qs}`);
  },

  async get(id: string) {
    return apiGet<Thing>(`/things/${id}`);
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
};
