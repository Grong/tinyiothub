/**
 * 设备模板 API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';
import type {
  Template,
  TemplateListParams,
  CreateTemplateRequest,
  UpdateTemplateRequest,
} from '../types/index.js';
import type { PaginatedResponse } from './client.js';

export const templateApi = {
  async getTemplates(params?: TemplateListParams) {
    return apiGet<PaginatedResponse<Template>>('/device-templates', params as Record<string, any>);
  },

  async getTemplate(id: string) {
    return apiGet<Template>(`/device-templates/${id}`);
  },

  async createTemplate(data: CreateTemplateRequest) {
    return apiPost<Template>('/device-templates', data);
  },

  async updateTemplate(id: string, data: Partial<UpdateTemplateRequest>) {
    return apiPut<Template>(`/device-templates/${id}`, data);
  },

  async deleteTemplate(id: string) {
    return apiDelete<void>(`/device-templates/${id}`);
  },
};
