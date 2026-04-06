/**
 * 标签 API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type {
  Tag,
  CreateTagRequest,
  UpdateTagRequest,
  CreateTagBindingRequest,
  BatchTagBindingRequest,
  TagStats,
} from '../types/index.js';

export const tagApi = {
  async getTags(params?: { type?: string }) {
    return apiGet<Tag[]>('/tags', params as Record<string, any>);
  },

  async getTag(id: string) {
    return apiGet<Tag>(`/tags/${id}`);
  },

  async createTag(data: CreateTagRequest) {
    return apiPost<Tag>('/tags', data);
  },

  async updateTag(id: string, data: UpdateTagRequest) {
    return apiPut<Tag>(`/tags/${id}`, data);
  },

  async deleteTag(id: string) {
    return apiDelete<void>(`/tags/${id}`);
  },

  async getTagStats() {
    return apiGet<TagStats>('/tags/stats');
  },

  async createBinding(data: CreateTagBindingRequest) {
    return apiPost<void>('/tags/bindings', data);
  },

  async batchBind(data: BatchTagBindingRequest) {
    return apiPost<void>('/tags/bindings/batch', data);
  },

  async removeBinding(bindingId: string) {
    return apiDelete<void>(`/tags/bindings/${bindingId}`);
  },
};
