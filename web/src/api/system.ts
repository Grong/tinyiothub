/**
 * 系统管理 API
 */

import { apiGet, apiPut } from './client.js';
import type { SystemConfig, SystemFeatures, SystemTask } from '../types/index.js';

export const systemApi = {
  async getConfigs(params?: { category?: string }) {
    return apiGet<SystemConfig[]>('/system/configs', params);
  },

  async updateConfig(key: string, value: string) {
    return apiPut<void>(`/system/configs/${key}`, { value });
  },

  async getFeatures() {
    return apiGet<SystemFeatures>('/system/features');
  },

  async getTasks(params?: { status?: string }) {
    return apiGet<SystemTask[]>('/system/tasks', params);
  },
};
