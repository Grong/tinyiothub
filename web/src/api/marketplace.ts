/**
 * 市场 API
 */

import { apiGet } from './client.js';

export const marketplaceApi = {
  async getItems(params?: { page?: number; pageSize?: number; category?: string }) {
    return apiGet<any>('/marketplace', params);
  },

  async getItem(id: string) {
    return apiGet<any>(`/marketplace/${id}`);
  },
};
