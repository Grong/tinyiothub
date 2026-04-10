/**
 * 驱动管理 API
 */

import { apiGet, apiPost, apiPut, apiDelete } from './client.js';

export const driverApi = {
  async getDrivers(params?: { page?: number; pageSize?: number }) {
    return apiGet<any>('/drivers', params);
  },

  async getDriver(id: string) {
    return apiGet<any>(`/drivers/${id}`);
  },

  async createDriver(data: any) {
    return apiPost<any>('/drivers', data);
  },

  async updateDriver(id: string, data: any) {
    return apiPut<any>(`/drivers/${id}`, data);
  },

  async deleteDriver(id: string) {
    return apiDelete<void>(`/drivers/${id}`);
  },

  async getDriverConfig(driverName: string) {
    return apiGet<any>(`/drivers/${driverName}/config`);
  },

  async getDriverNames() {
    return apiGet<string[]>('/drivers/names');
  },
};
